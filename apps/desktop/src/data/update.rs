//! Updates from the GitHub releases. Each release carries `latest.json`, which names the version
//! and each platform's archive with its SHA-256, and `latest.json.sig`, an Ed25519 signature of
//! it made by the release workflow. Mrow fetches both with `curl`, checks the signature against
//! the key it was built with, downloads and checks its platform's archive, unpacks it into the
//! cache folder, and puts it in place when the person restarts.
//!
//! Only builds made by the release workflow (`MROW_UPDATES` set at build time) update
//! themselves; anything built from source updates from git.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};

use rune::Waker;
use serde::Deserialize;
use sha2::Digest;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENABLED: bool = option_env!("MROW_UPDATES").is_some();
pub const RELEASES: &str = "https://github.com/ZainW/mrow/releases";
/// The release workflow's signing key (`packaging/update-key.hex`).
const PUBLIC_KEY: &str = include_str!("../../packaging/update-key.hex");

/// Seconds after launch before the first check, then between checks.
const FIRST_CHECK: f64 = 20.0;
const CHECK_EVERY: f64 = 6.0 * 3600.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Idle,
    Checking,
    Current,
    Downloading(String),
    Ready(Ready),
    Failed(String),
}

/// An update unpacked and waiting for a restart.
#[derive(Clone, Debug, PartialEq)]
pub struct Ready {
    pub version: String,
    pub notes_url: String,
    /// `Mrow.app` on macOS, the `mrow` binary elsewhere.
    payload: PathBuf,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    notes_url: String,
    assets: BTreeMap<String, Asset>,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
    sha256: String,
}

pub struct Updater {
    /// The worker's latest phase, and a count that moves when it changes.
    shared: Arc<Mutex<(u64, Phase)>>,
    seen: u64,
    waker: Option<Waker>,
    pub phase: Phase,
    /// When the next automatic check runs, on the frame clock (0 before the first poll).
    next_check: f64,
    /// Whether the running check was asked for, so its outcome is reported either way.
    manual: bool,
    /// Headless runs never check.
    pub offline: bool,
}

impl Default for Updater {
    fn default() -> Self {
        Updater { shared: Arc::new(Mutex::new((0, Phase::Idle))), seen: 0, waker: None, phase: Phase::Idle, next_check: 0.0, manual: false, offline: false }
    }
}

impl Updater {
    /// For the `update` scene: an update downloaded and waiting.
    pub fn insert_fixture(&mut self, version: &str) {
        self.phase = Phase::Ready(Ready { version: version.into(), notes_url: format!("{RELEASES}/tag/v{version}"), payload: PathBuf::new() });
    }

    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Checking | Phase::Downloading(_))
    }

    pub fn ready(&self) -> Option<&Ready> {
        match &self.phase {
            Phase::Ready(r) => Some(r),
            _ => None,
        }
    }

    /// Takes the worker's progress and runs the automatic checks. Returns whether the phase
    /// changed, a line to show the person, and when to wake for the next check.
    pub fn poll(&mut self, clock: f64, automatic: bool) -> (bool, Option<String>, Option<f64>) {
        if !ENABLED || self.offline {
            return (false, None, None);
        }
        if self.next_check == 0.0 {
            self.next_check = clock + FIRST_CHECK;
            // Whatever an earlier run unpacked is either in place now or out of date.
            let _ = std::fs::remove_dir_all(staging_root());
        }
        let mut changed = false;
        let mut say = None;
        let (count, phase) = self.shared.lock().unwrap().clone();
        if count != self.seen {
            self.seen = count;
            changed = true;
            say = match &phase {
                Phase::Ready(r) if !matches!(self.phase, Phase::Ready(_)) => Some(format!("Mrow {} is ready. Restart to update.", r.version)),
                Phase::Current if self.manual => Some(format!("Mrow {VERSION} is the latest version.")),
                Phase::Failed(e) if self.manual => Some(format!("Couldn't check for updates: {e}")),
                _ => None,
            };
            if !matches!(phase, Phase::Checking | Phase::Downloading(_)) {
                self.manual = false;
            }
            self.phase = phase;
        }
        if automatic && clock >= self.next_check {
            self.next_check = clock + CHECK_EVERY;
            if !self.busy() && self.ready().is_none() {
                self.start();
                changed = true;
            }
        }
        (changed, say, automatic.then_some(self.next_check))
    }

    /// A check asked for from the menu or Settings. Returns a line to show when there's no
    /// check to run.
    pub fn check_now(&mut self) -> Option<String> {
        if !ENABLED {
            return Some("This copy of Mrow was built from source, so it updates from git.".into());
        }
        if let Some(r) = self.ready() {
            return Some(format!("Mrow {} is ready. Restart to update.", r.version));
        }
        self.manual = true;
        if !self.busy() {
            self.start();
        }
        None
    }

    fn start(&mut self) {
        self.phase = Phase::Checking;
        let shared = self.shared.clone();
        let waker = self.waker.clone();
        let set = move |phase: Phase| {
            let mut s = shared.lock().unwrap();
            s.0 += 1;
            s.1 = phase;
            drop(s);
            if let Some(w) = &waker {
                w();
            }
        };
        set(Phase::Checking);
        std::thread::spawn(move || {
            let phase = match fetch(&set) {
                Ok(phase) => phase,
                Err(e) => Phase::Failed(e),
            };
            set(phase);
        });
    }
}

/// Finds out whether there's something newer and, if so, downloads, checks and unpacks it.
fn fetch(set: &dyn Fn(Phase)) -> Result<Phase, String> {
    let latest = format!("{}/latest.json", feed());
    let manifest = download(&latest)?;
    let signature = download(&format!("{latest}.sig"))?;
    verify(&manifest, &signature)?;
    let manifest: Manifest = serde_json::from_slice(&manifest).map_err(|e| format!("the release's manifest didn't read: {e}"))?;
    if !newer(&manifest.version, VERSION) {
        return Ok(Phase::Current);
    }
    let key = platform();
    let asset = manifest.assets.get(&key).ok_or_else(|| format!("{} has no build for {key}", manifest.version))?;
    set(Phase::Downloading(manifest.version.clone()));
    let archive = download(&asset.url)?;
    if hex(&sha2::Sha256::digest(&archive)) != asset.sha256.to_lowercase() {
        return Err("the download didn't match its checksum".into());
    }
    let payload = unpack(&manifest.version, &archive)?;
    Ok(Phase::Ready(Ready { version: manifest.version, notes_url: manifest.notes_url, payload }))
}

/// Where `latest.json` lives. `MROW_UPDATE_FEED` points elsewhere (a `file://` folder) to try a
/// release before it's published; the manifest still has to carry a valid signature.
fn feed() -> String {
    std::env::var("MROW_UPDATE_FEED").unwrap_or_else(|_| format!("{RELEASES}/latest/download"))
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let proto = if std::env::var_os("MROW_UPDATE_FEED").is_some() { "=https,file" } else { "=https" };
    let out = Command::new("/usr/bin/curl")
        .args(["-sSfL", "--proto", proto, "--proto-redir", "=https", "--max-time", "600", "-A", concat!("Mrow/", env!("CARGO_PKG_VERSION")), url])
        .output()
        .map_err(|e| format!("couldn't run curl: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().trim_start_matches("curl: ").to_string();
        return Err(if err.contains("404") { "no release is published yet".into() } else { err });
    }
    Ok(out.stdout)
}

/// Checks `latest.json` against the signature the release workflow made of it.
fn verify(manifest: &[u8], signature: &[u8]) -> Result<(), String> {
    use ed25519_dalek::{Signature, VerifyingKey};
    let bad = || "the release's signature didn't check out".to_string();
    let key: [u8; 32] = unhex(PUBLIC_KEY.trim()).and_then(|k| k.try_into().ok()).expect("packaging/update-key.hex holds a 32-byte key");
    let key = VerifyingKey::from_bytes(&key).map_err(|_| bad())?;
    let signature: [u8; 64] = unhex(String::from_utf8_lossy(signature).trim()).and_then(|s| s.try_into().ok()).ok_or_else(bad)?;
    key.verify_strict(manifest, &Signature::from_bytes(&signature)).map_err(|_| bad())
}

/// Unpacks the archive into the cache folder and returns what goes in place.
fn unpack(version: &str, archive: &[u8]) -> Result<PathBuf, String> {
    let dir = staging_root().join(version);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let file = dir.join("update.tar.gz");
    std::fs::write(&file, archive).map_err(|e| format!("couldn't save the download: {e}"))?;
    let out = Command::new("tar").arg("-xzf").arg(&file).arg("-C").arg(&dir).output().map_err(|e| format!("couldn't run tar: {e}"))?;
    if !out.status.success() {
        return Err(format!("couldn't unpack the download: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let _ = std::fs::remove_file(&file);
    let payload = if cfg!(target_os = "macos") { dir.join("Mrow.app") } else { dir.join(format!("mrow-linux-{}", std::env::consts::ARCH)).join("bin/mrow") };
    if !payload.exists() {
        return Err("the download didn't hold the app".into());
    }
    Ok(payload)
}

fn staging_root() -> PathBuf {
    super::paths::cache_dir().join("updates")
}

/// The manifest's name for this build: "macos-aarch64", "linux-x86_64"…
fn platform() -> String {
    format!("{}-{}", if cfg!(target_os = "macos") { "macos" } else { "linux" }, std::env::consts::ARCH)
}

/// Whether `candidate` is a later "major.minor.patch" than `current`.
pub fn newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> Option<(u64, u64, u64)> {
        let mut it = v.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok());
        let parts = (it.next()??, it.next()??, it.next()??);
        it.next().is_none().then_some(parts)
    }
    matches!((parts(candidate), parts(current)), (Some(a), Some(b)) if a > b)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// `mrow --update`: checks, downloads and puts a newer version in place, from a terminal.
pub fn run_cli() -> i32 {
    if !ENABLED {
        println!("This copy of Mrow was built from source, so it updates from git.");
        return 1;
    }
    remember_exe();
    let progress = |phase: Phase| {
        if let Phase::Downloading(v) = phase {
            println!("Downloading {v}…");
        }
    };
    match fetch(&progress) {
        Ok(Phase::Ready(ready)) => match install(&ready) {
            Ok(()) => {
                println!("Updated to {}. Quit and reopen Mrow if it's running.", ready.version);
                0
            }
            Err(e) => {
                println!("{e}");
                1
            }
        },
        Ok(_) => {
            println!("Mrow {VERSION} is the latest version.");
            0
        }
        Err(e) => {
            println!("Couldn't update: {e}");
            1
        }
    }
}

// MARK: Putting it in place

/// The running binary, found at launch: once it has been replaced, Linux reports its path
/// with " (deleted)" on the end.
static EXE: LazyLock<Option<PathBuf>> = LazyLock::new(|| std::env::current_exe().ok().and_then(|p| p.canonicalize().ok()));

/// Notes where the running binary is, before anything can replace it.
pub fn remember_exe() {
    LazyLock::force(&EXE);
}

/// Puts the update in place of the running app. The caller restarts it.
pub fn install(ready: &Ready) -> Result<(), String> {
    let exe = EXE.as_ref().ok_or("couldn't tell where Mrow is running from")?;
    #[cfg(target_os = "macos")]
    let target = exe.ancestors().nth(3).filter(|b| b.extension().is_some_and(|e| e == "app")).ok_or("Mrow isn't running from an app bundle")?.to_path_buf();
    #[cfg(not(target_os = "macos"))]
    let target = exe.clone();
    replace(&ready.payload, &target).map_err(|e| {
        let folder = target.parent().unwrap_or(Path::new("/")).display().to_string();
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            format!("Mrow can't write to {folder}. Download {} from the releases page instead.", ready.version)
        } else {
            format!("Couldn't put the update in place: {e}")
        }
    })
}

/// Copies `new` beside `target`, then swaps it in with renames, so a failure part-way leaves
/// the old copy where it was.
fn replace(new: &Path, target: &Path) -> std::io::Result<()> {
    let parent = target.parent().ok_or_else(|| std::io::Error::other("no parent folder"))?;
    let name = target.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let incoming = parent.join(format!(".{name}.incoming"));
    let outgoing = parent.join(format!(".{name}.outgoing"));
    remove(&incoming)?;
    remove(&outgoing)?;
    // ditto keeps the bundle's symlinks, permissions and signature; a binary is one file.
    let copied = if new.is_dir() { Command::new("/usr/bin/ditto").arg(new).arg(&incoming).status()?.success() } else { std::fs::copy(new, &incoming).is_ok() };
    if !copied {
        let _ = remove(&incoming);
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "copy failed"));
    }
    #[cfg(unix)]
    if new.is_file() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&incoming, std::fs::Permissions::from_mode(0o755))?;
    }
    if new.is_dir() {
        std::fs::rename(target, &outgoing)?;
        if let Err(e) = std::fs::rename(&incoming, target) {
            let _ = std::fs::rename(&outgoing, target);
            return Err(e);
        }
        let _ = remove(&outgoing);
    } else {
        // A rename over a running binary is fine: the old one lives on until this process ends.
        std::fs::rename(&incoming, target)?;
    }
    Ok(())
}

fn remove(path: &Path) -> std::io::Result<()> {
    let result = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Off macOS, starts the new binary to take over once this process quits. (Rune's own relaunch
/// would look up the running binary, which is the replaced one by now.) macOS relaunches the
/// bundle through Rune.
#[cfg(not(target_os = "macos"))]
pub fn start_replacement() -> Result<(), String> {
    let exe = EXE.as_ref().ok_or("couldn't tell where Mrow is running from")?;
    Command::new(exe)
        .args(std::env::args_os().skip(1))
        // Rune's relaunch handshake: the new process waits for this one to exit.
        .env("RUNE_RELAUNCH_AFTER", std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("couldn't start the new version: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("v1.0.0", "0.99.0"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0"));
        assert!(!newer("0.2", "0.1.0"));
        assert!(!newer("0.2.0-beta", "0.1.0"));
    }

    #[test]
    fn the_key_is_32_bytes() {
        assert_eq!(unhex(PUBLIC_KEY.trim()).map(|k| k.len()), Some(32));
    }

    #[test]
    fn a_forged_manifest_is_refused() {
        let bad = "00".repeat(64);
        assert!(verify(b"{\"version\":\"9.9.9\"}", bad.as_bytes()).is_err());
        assert!(verify(b"{}", b"not hex").is_err());
    }

    /// `MROW_SIGNED=<file>` with `<file>.sig` beside it, signed as the release workflow signs.
    #[test]
    #[ignore]
    fn a_signed_file_verifies() {
        let path = std::env::var("MROW_SIGNED").expect("MROW_SIGNED");
        let data = std::fs::read(&path).unwrap();
        let sig = std::fs::read(format!("{path}.sig")).unwrap();
        verify(&data, &sig).unwrap();
        let mut tampered = data.clone();
        tampered.push(b' ');
        assert!(verify(&tampered, &sig).is_err());
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(unhex(&hex(&[0, 1, 0xab, 0xff])), Some(vec![0, 1, 0xab, 0xff]));
        assert_eq!(unhex("abc"), None);
    }
}
