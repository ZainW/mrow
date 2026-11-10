//! Where Mrow keeps its files and looks for the tools it runs, per platform: the Apple folders
//! under `~/Library` on macOS, the XDG folders elsewhere.

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// An XDG base directory: the variable when it's an absolute path, else `fallback` under home.
#[cfg(not(target_os = "macos"))]
fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(fallback))
}

/// Settings and the image cache.
pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    return home().join("Library/Application Support/Mrow");
    #[cfg(not(target_os = "macos"))]
    return xdg("XDG_CONFIG_HOME", ".config").join("mrow");
}

/// Scratch space, such as the empty folder an agent chat runs in.
pub fn cache_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    return home().join("Library/Caches/Mrow");
    #[cfg(not(target_os = "macos"))]
    return xdg("XDG_CACHE_HOME", ".cache").join("mrow");
}

/// Folders a package manager installs CLIs into. An app opened from Finder or a desktop
/// launcher gets a minimal PATH, so these are searched first and added to children's PATH.
pub fn tool_dirs() -> Vec<PathBuf> {
    let home = home();
    let mut dirs = vec![PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin"), home.join(".local/bin")];
    if cfg!(not(target_os = "macos")) {
        dirs.extend([PathBuf::from("/home/linuxbrew/.linuxbrew/bin"), home.join(".linuxbrew/bin"), PathBuf::from("/usr/bin"), PathBuf::from("/snap/bin")]);
    }
    dirs.extend([PathBuf::from("/run/current-system/sw/bin"), home.join(".nix-profile/bin")]);
    dirs
}

/// `PATH` for a child process: ours, then the tool folders.
pub fn child_path() -> String {
    let extra = tool_dirs().iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>().join(":");
    match std::env::var("PATH") {
        Ok(p) if !p.is_empty() => format!("{p}:{extra}"),
        _ => extra,
    }
}

/// The user's login shell, to ask where a command lives as their terminal would.
pub fn login_shell() -> String {
    std::env::var("SHELL").ok().filter(|s| s.starts_with('/')).unwrap_or_else(|| if cfg!(target_os = "macos") { "/bin/zsh".into() } else { "/bin/sh".into() })
}

/// How to install a forge CLI, for the error shown when it's missing.
pub fn install_hint(name: &str) -> String {
    if cfg!(target_os = "macos") { format!("brew install {name}") } else { format!("install {name} with your package manager") }
}
