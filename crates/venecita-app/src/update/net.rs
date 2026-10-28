//! The network side of updating (macOS and Windows): asking GitHub for the latest
//! release, downloading a package with its `.sha256` into a fresh folder, verifying it and
//! opening it. Everything here blocks: the settings window runs it on its own thread.
//!
//! Every request (and every redirect, which are followed by hand) must pass
//! [`is_allowed_url`]; a package is only opened after its SHA-256 matched.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::{
    is_allowed_url, parse_release, parse_sha256, redact_url, to_hex, Package, Release, UpdateError,
    CURRENT_VERSION, LATEST_RELEASE_URL,
};

/// Redirects followed per request (GitHub uses one or two for downloads).
const MAX_REDIRECTS: usize = 5;
/// No package is anywhere near this big; a larger answer is refused.
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
/// Limit for the JSON answer and the `.sha256` file.
const MAX_TEXT_BYTES: u64 = 1024 * 1024;

fn agent() -> Result<ureq::Agent, UpdateError> {
    let tls = ureq::native_tls::TlsConnector::new()
        .map_err(|e| UpdateError::Network(format!("TLS: {e}")))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .https_only(true)
        // Redirects are followed in `get`, checking each target.
        .redirects(0)
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .user_agent(&format!("venecita-dock/{CURRENT_VERSION}"))
        .build())
}

/// GET `url`, following redirects only to allowed hosts.
fn get(agent: &ureq::Agent, url: &str, accept: &str) -> Result<ureq::Response, UpdateError> {
    let mut url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !is_allowed_url(&url) {
            tracing::warn!(url = %redact_url(&url), "update: refusing a URL outside GitHub");
            return Err(UpdateError::UrlNotAllowed(url));
        }
        let response = match agent.get(&url).set("Accept", accept).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(code, response)) => {
                let out_of_quota = response.header("x-ratelimit-remaining") == Some("0");
                tracing::warn!(code, url = %redact_url(&url), "update: HTTP error");
                return Err(if code == 429 || (code == 403 && out_of_quota) {
                    UpdateError::RateLimited
                } else {
                    UpdateError::Http(code)
                });
            }
            Err(ureq::Error::Transport(e)) => {
                // Not `e.to_string()`: it includes the URL, and download URLs are signed.
                let reason = match e.message() {
                    Some(message) => format!("{}: {message}", e.kind()),
                    None => e.kind().to_string(),
                };
                tracing::warn!(error = %reason, url = %redact_url(&url), "update: network error");
                return Err(UpdateError::Network(reason));
            }
        };
        if !(300..400).contains(&response.status()) {
            return Ok(response);
        }
        let location = response
            .header("location")
            .ok_or(UpdateError::BadResponse)?;
        // The location may be relative to the URL that answered.
        url = url::Url::parse(&url)
            .and_then(|base| base.join(location))
            .map_err(|_| UpdateError::BadResponse)?
            .to_string();
        tracing::debug!(to = %redact_url(&url), "update: redirected");
    }
    Err(UpdateError::BadResponse)
}

fn read_text(response: ureq::Response) -> Result<String, UpdateError> {
    let mut text = String::new();
    response
        .into_reader()
        .take(MAX_TEXT_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    Ok(text)
}

/// Asks GitHub for the latest stable release.
pub fn fetch_latest() -> Result<Release, UpdateError> {
    let agent = agent()?;
    let response = get(&agent, LATEST_RELEASE_URL, "application/vnd.github+json")?;
    let release = parse_release(&read_text(response)?)?;
    tracing::info!(tag = %release.tag, current = CURRENT_VERSION, "update: latest release");
    Ok(release)
}

/// Downloads `package` and its checksum into a new folder in the user's temp directory,
/// verifies it and returns the file's path. `progress(done, total)` is called as bytes
/// arrive. A file that doesn't match its checksum is deleted.
pub fn download(
    package: &Package,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<PathBuf, UpdateError> {
    let agent = agent()?;

    let checksum = read_text(get(
        &agent,
        &package.checksum.url,
        "application/octet-stream",
    )?)?;
    let expected = parse_sha256(&checksum, &package.file.name).ok_or(UpdateError::BadResponse)?;

    let dir = fresh_dir()?;
    // The asset name comes from GitHub: keep only a plain file name.
    let name = Path::new(&package.file.name)
        .file_name()
        .filter(|n| *n == package.file.name.as_str())
        .ok_or(UpdateError::BadResponse)?;
    let path = dir.join(name);

    let response = get(&agent, &package.file.url, "application/octet-stream")?;
    let total = response
        .header("content-length")
        .and_then(|n| n.parse::<u64>().ok())
        .or(package.file.size);
    if total.is_some_and(|t| t > MAX_PACKAGE_BYTES) {
        return Err(UpdateError::TooLarge);
    }
    let actual = match save(response, &path, total, progress) {
        Ok(hash) => hash,
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
    };
    if actual != expected {
        tracing::warn!(file = %package.file.name, %expected, %actual, "update: SHA-256 mismatch, deleting the download");
        let _ = std::fs::remove_file(&path);
        return Err(UpdateError::HashMismatch);
    }
    tracing::info!(file = %path.display(), sha256 = %actual, "update: downloaded and verified");
    Ok(path)
}

/// Streams the body to a new file, returning its SHA-256.
fn save(
    response: ureq::Response,
    path: &Path,
    total: Option<u64>,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<String, UpdateError> {
    let file_error = |e: std::io::Error| UpdateError::File(e.to_string());
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(file_error)?;
    let mut reader = response.into_reader().take(MAX_PACKAGE_BYTES + 1);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    progress(0, total);
    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > MAX_PACKAGE_BYTES {
            return Err(UpdateError::TooLarge);
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n]).map_err(file_error)?;
        progress(done, total);
    }
    file.sync_all().map_err(file_error)?;
    Ok(to_hex(&hasher.finalize()))
}

/// A new, empty folder only this user can write to: `<temp>/venecita-dock-update-<random>`.
/// Created with `create_dir`, so an existing folder (or link) of that name is never reused.
fn fresh_dir() -> Result<PathBuf, UpdateError> {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let base = std::env::temp_dir();
    let mut last_error = None;
    for attempt in 0..8u32 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        hasher.write_u32(attempt);
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(now.as_nanos());
        }
        let dir = base.join(format!("venecita-dock-update-{:016x}", hasher.finish()));
        // On Windows the user's %TEMP% is already private to them.
        #[cfg(windows)]
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last_error = Some(e),
            Err(e) => return Err(UpdateError::File(e.to_string())),
        }
    }
    Err(UpdateError::File(last_error.map_or_else(
        || "temp folder".to_string(),
        |e| e.to_string(),
    )))
}

/// Runs the verified Windows installer silently. It closes the tray app, updates it in
/// place and starts it again; the settings window should close right after.
#[cfg(windows)]
pub fn install(path: &Path) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    if !path.is_absolute() {
        return Err(UpdateError::Launch("not an absolute path".into()));
    }
    // `/SUPPRESSMSGBOXES`: no message box may stop an unattended update (the running app is
    // closed by the installer itself, see installer/venecita.iss).
    std::process::Command::new(path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES"])
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| tracing::info!(installer = %path.display(), "update: installer started"))
        .map_err(|e| UpdateError::Launch(e.to_string()))
}

/// Opens the verified DMG in Finder: the user drags the new app over the old one.
#[cfg(target_os = "macos")]
pub fn install(path: &Path) -> Result<(), UpdateError> {
    std::process::Command::new("/usr/bin/open")
        .arg(path)
        .spawn()
        .map(|_| tracing::info!(dmg = %path.display(), "update: disk image opened"))
        .map_err(|e| UpdateError::Launch(e.to_string()))
}

/// Opens a release page in the browser, if it is a GitHub URL.
pub fn open_page(url: &str) {
    if !is_allowed_url(url) {
        tracing::warn!(url = %redact_url(url), "update: not opening a URL outside GitHub");
        return;
    }
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("/usr/bin/open").arg(url).spawn();
    // No shell in between: the URL is passed as one argument.
    #[cfg(windows)]
    let spawned = std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "update: cannot open the release page");
    }
}
