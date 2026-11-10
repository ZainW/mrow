//! Where a picture in a description or comment comes from, and fetching it. GitLab uploads sit
//! behind the user's login, so they go through `glab api` (which holds the token); anything
//! else is a plain download with `curl`. Fetched pictures are kept on disk.

use std::path::PathBuf;
use std::process::Command;

use crate::data::cli::Cli;
use crate::data::models::fnv1a;
use crate::data::prefs;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageSource {
    Upload { project: String, secret: String, file: String },
    Web(String),
}

impl ImageSource {
    /// `/uploads/<secret>/<file>` (relative to the project), `/-/project/<id>/uploads/…`, or a
    /// full URL, which is an upload too when it's on the project's host.
    pub fn resolve(raw: &str, base: Option<&str>) -> Option<ImageSource> {
        let raw = raw.trim();
        let (origin, project) = match base {
            Some(b) => split_url(b),
            None => (String::new(), String::new()),
        };
        if raw.starts_with("/uploads/") || raw.starts_with("uploads/") {
            if project.is_empty() {
                return None;
            }
            return upload(&project, raw.trim_start_matches('/'));
        }
        if let Some(rest) = raw.strip_prefix("/-/project/") {
            let (id, path) = rest.split_once('/')?;
            return upload(id, path);
        }
        let lower = raw.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return None;
        }
        let (url_origin, url_path) = split_url(raw);
        if !origin.is_empty() && url_origin.eq_ignore_ascii_case(&origin) {
            if let Some(at) = url_path.find("/uploads/").or_else(|| url_path.starts_with("uploads/").then_some(0)) {
                let mut owner = url_path[..at].trim_matches('/').to_string();
                if let Some(o) = owner.strip_prefix("-/project/") {
                    owner = o.to_string();
                }
                if let Some(o) = owner.strip_suffix("/-") {
                    owner = o.to_string();
                }
                let tail = url_path[at..].trim_start_matches('/');
                if !owner.is_empty() {
                    if let Some(source) = upload(&owner, tail) {
                        return Some(source);
                    }
                }
            }
        }
        Some(ImageSource::Web(raw.to_string()))
    }

    pub fn key(&self) -> String {
        match self {
            ImageSource::Upload { project, secret, file } => format!("upload|{project}|{secret}|{file}"),
            ImageSource::Web(url) => format!("web|{url}"),
        }
    }

    /// Downloads the picture (blocking), from the disk cache when it's there.
    pub fn fetch(&self) -> Option<Vec<u8>> {
        let path = disk_path(self);
        if let Ok(data) = std::fs::read(&path) {
            return Some(data);
        }
        let data = match self {
            ImageSource::Upload { project, secret, file } => {
                let id = if project.chars().all(|c| c.is_ascii_digit()) { project.clone() } else { encode(project, false) };
                let name = encode(file, true);
                let data = Cli::glab().api(&[format!("projects/{id}/uploads/{secret}/{name}")], None).ok()?;
                // A missing upload comes back as a JSON error rather than a failure.
                if data.is_empty() || data[0] == b'{' {
                    return None;
                }
                data
            }
            ImageSource::Web(url) => {
                let out = Command::new("/usr/bin/curl").args(["-sSfL", "--max-time", "30", "-A", "Mrow", url]).output().ok()?;
                if !out.status.success() || out.stdout.is_empty() {
                    return None;
                }
                out.stdout
            }
        };
        // Uploads are kept; anything on the web can change, so it's fetched each launch.
        if let (ImageSource::Upload { .. }, Some(dir)) = (self, path.parent()) {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&path, &data);
        }
        Some(data)
    }
}

/// "https://host/group/project" as ("https://host", "group/project").
fn split_url(url: &str) -> (String, String) {
    let Some(scheme) = url.find("://") else { return (String::new(), String::new()) };
    let rest = &url[scheme + 3..];
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.split(['?', '#']).next().unwrap_or("");
    (format!("{}{}", &url[..scheme + 3], host), path.trim_matches('/').to_string())
}

/// uploads/<secret>/<file>
fn upload(project: &str, path: &str) -> Option<ImageSource> {
    let mut parts = path.splitn(3, '/');
    let (Some("uploads"), Some(secret), Some(file)) = (parts.next(), parts.next(), parts.next()) else { return None };
    if secret.is_empty() || file.is_empty() {
        return None;
    }
    Some(ImageSource::Upload { project: project.to_string(), secret: secret.to_string(), file: decode(file) })
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Percent-encodes everything but unreserved characters (and `/` in a path when allowed).
fn encode(s: &str, keep_dots: bool) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let keep = b.is_ascii_alphanumeric() || (keep_dots && matches!(b, b'.' | b'-' | b'_' | b'~'));
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn disk_path(source: &ImageSource) -> PathBuf {
    prefs::folder().join("Images").join(format!("{:016x}", fnv1a(&source.key()) as u64))
}
