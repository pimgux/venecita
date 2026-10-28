//! Updating to the latest published version (Settings › General › Updates).
//!
//! This module is the pure part, covered by unit tests: comparing versions, reading
//! GitHub's "latest release" answer, choosing the package for this system, reading a
//! `.sha256` file and deciding which URLs may be fetched at all. The network side
//! (`net`, macOS and Windows) downloads with those rules and verifies before running.
//!
//! Only stable releases count: GitHub's `releases/latest` endpoint leaves pre-releases
//! (tags with `-`) out, and [`parse_release`] rejects one if it ever came back.

use std::cmp::Ordering;
use std::fmt;

use serde::Deserialize;

#[cfg(any(target_os = "macos", windows))]
pub mod net;

/// The version of this build, e.g. `"0.1.3"`.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub's API answer with the latest stable release.
pub const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/pimgux/venecita/releases/latest";

/// The releases page, for when there is nothing better to open.
pub const RELEASES_PAGE_URL: &str = "https://github.com/pimgux/venecita/releases/latest";

/// Hosts the updater talks to: the API, the release pages and the servers GitHub
/// redirects asset downloads to. Anything else is refused, redirects included.
pub const ALLOWED_HOSTS: [&str; 4] = [
    "api.github.com",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// A version like `1.2.3` (a leading `v` and a `+build` suffix are ignored).
///
/// A pre-release (`1.2.3-alpha.1`) sorts before its release; two pre-releases of the
/// same version compare equal (they are never offered as updates anyway).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// The first number.
    pub major: u64,
    /// The second number.
    pub minor: u64,
    /// The third number.
    pub patch: u64,
    /// It has a `-something` suffix.
    pub pre_release: bool,
}

impl Version {
    /// Parses `"v1.2.3"`, `"1.2.3"`, `"1.2"` (= 1.2.0), `"1.2.3-beta"`… `None` if it isn't
    /// one.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text
            .strip_prefix('v')
            .or_else(|| text.strip_prefix('V'))
            .unwrap_or(text);
        let text = text.split_once('+').map_or(text, |(v, _build)| v);
        let (numbers, pre_release) = match text.split_once('-') {
            Some((numbers, pre)) if !pre.is_empty() => (numbers, true),
            Some(_) => return None,
            None => (text, false),
        };
        let mut parts = [0u64; 3];
        for (i, part) in numbers.split('.').enumerate() {
            if i == 3 || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            parts[i] = part.parse().ok()?;
        }
        Some(Self {
            major: parts[0],
            minor: parts[1],
            patch: parts[2],
            pre_release,
        })
    }

    /// The version of this build.
    pub fn current() -> Self {
        Self::parse(CURRENT_VERSION).expect("the crate version is a valid version")
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch, !self.pre_release).cmp(&(
            other.major,
            other.minor,
            other.patch,
            !other.pre_release,
        ))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only the numbers: pre-releases are never offered.
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// File name, e.g. `venecita-dock-v0.1.4-macos.dmg`.
    pub name: String,
    /// Where to download it from.
    pub url: String,
    /// Size in bytes, as GitHub reports it.
    pub size: Option<u64>,
}

/// The latest published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The tag, e.g. `v0.1.4`.
    pub tag: String,
    /// The version in the tag.
    pub version: Version,
    /// The release's web page (notes and downloads).
    pub page_url: String,
    /// Its files.
    pub assets: Vec<Asset>,
}

impl Release {
    /// Whether this release is newer than `current`.
    pub fn is_newer_than(&self, current: Version) -> bool {
        self.version > current
    }

    /// The release page if its URL is one we may open, else the generic releases page.
    pub fn safe_page_url(&self) -> &str {
        if is_allowed_url(&self.page_url) {
            &self.page_url
        } else {
            RELEASES_PAGE_URL
        }
    }
}

#[derive(Deserialize)]
struct ReleaseJson {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Deserialize)]
struct AssetJson {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Reads the JSON of `GET /repos/{owner}/{repo}/releases/latest`.
pub fn parse_release(json: &str) -> Result<Release, UpdateError> {
    let raw: ReleaseJson = serde_json::from_str(json).map_err(|_| UpdateError::BadResponse)?;
    if raw.prerelease || raw.draft {
        return Err(UpdateError::BadResponse);
    }
    let version = Version::parse(&raw.tag_name).ok_or(UpdateError::BadResponse)?;
    if version.pre_release {
        return Err(UpdateError::BadResponse);
    }
    Ok(Release {
        tag: raw.tag_name,
        version,
        page_url: raw.html_url,
        assets: raw
            .assets
            .into_iter()
            .map(|a| Asset {
                name: a.name,
                url: a.browser_download_url,
                size: a.size,
            })
            .collect(),
    })
}

/// The systems the updater knows how to install on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// A per-user Inno Setup installer, run silently.
    Windows,
    /// A DMG the user opens and drags the app from.
    MacOs,
    /// No package: the release page.
    Other,
}

impl Os {
    /// This system.
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Other
        }
    }
}

/// The name of the package for `os` in the release tagged `tag` (as the "Publicar
/// versión" workflow names them).
pub fn package_name(os: Os, tag: &str) -> Option<String> {
    match os {
        Os::Windows => Some(format!("venecita-dock-{tag}-windows-x86_64-setup.exe")),
        Os::MacOs => Some(format!("venecita-dock-{tag}-macos.dmg")),
        Os::Other => None,
    }
}

/// A package and the `.sha256` file that vouches for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// The installer or disk image.
    pub file: Asset,
    /// `<file>.sha256`.
    pub checksum: Asset,
}

/// The package for `os` in `release`, if it was published together with its checksum.
/// Without a checksum there is nothing to verify against, so no package.
pub fn select_package(release: &Release, os: Os) -> Option<Package> {
    let name = package_name(os, &release.tag)?;
    let find = |name: &str| release.assets.iter().find(|a| a.name == name).cloned();
    Some(Package {
        file: find(&name)?,
        checksum: find(&format!("{name}.sha256"))?,
    })
}

/// Reads a `.sha256` file (`sha256sum` output: `<hex>  <file name>`, or just `<hex>`)
/// and returns the lowercase hash for `file_name`.
pub fn parse_sha256(text: &str, file_name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let hash = words.next()?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        // `sha256sum -b` marks binary mode with a `*` before the name.
        match words.next().map(|n| n.trim_start_matches('*')) {
            None => Some(hash.to_ascii_lowercase()),
            Some(name) if name == file_name => Some(hash.to_ascii_lowercase()),
            Some(_) => None,
        }
    })
}

/// Lowercase hex of a digest.
pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Whether the updater may fetch (or open) `url`: HTTPS on the default port, one of
/// [`ALLOWED_HOSTS`], no credentials.
pub fn is_allowed_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| ALLOWED_HOSTS.contains(&host))
}

/// `url` without its query and fragment, for logs and messages: GitHub's download
/// redirects carry signed, short-lived tokens there.
pub fn redact_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut url) => {
            url.set_query(None);
            url.set_fragment(None);
            let _ = url.set_password(None);
            let _ = url.set_username("");
            url.to_string()
        }
        Err(_) => "(invalid URL)".to_string(),
    }
}

/// Why checking or updating didn't work, in terms the settings window can explain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// No connection (DNS, TLS, timeout…).
    Network(String),
    /// GitHub's API limit for anonymous requests (403/429).
    RateLimited,
    /// Another HTTP error status.
    Http(u16),
    /// The answer wasn't what a release looks like.
    BadResponse,
    /// A URL (or a redirect) outside [`ALLOWED_HOSTS`].
    UrlNotAllowed(String),
    /// The download doesn't match its `.sha256`: it is deleted, never run.
    HashMismatch,
    /// The download is larger than any package should be.
    TooLarge,
    /// Writing the download failed.
    File(String),
    /// The installer or disk image couldn't be opened.
    Launch(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::Network(e) => write!(f, "network error: {e}"),
            UpdateError::RateLimited => write!(f, "rate limited by GitHub"),
            UpdateError::Http(code) => write!(f, "HTTP status {code}"),
            UpdateError::BadResponse => write!(f, "unexpected answer"),
            UpdateError::UrlNotAllowed(url) => write!(f, "URL not allowed: {}", redact_url(url)),
            UpdateError::HashMismatch => write!(f, "SHA-256 mismatch"),
            UpdateError::TooLarge => write!(f, "download too large"),
            UpdateError::File(e) => write!(f, "file error: {e}"),
            UpdateError::Launch(e) => write!(f, "cannot open: {e}"),
        }
    }
}

impl std::error::Error for UpdateError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    #[test]
    fn versions_parse() {
        assert_eq!(
            v("v0.1.3"),
            Version {
                major: 0,
                minor: 1,
                patch: 3,
                pre_release: false
            }
        );
        assert_eq!(v("1.2"), v("1.2.0"));
        assert_eq!(v("V2"), v("2.0.0"));
        assert_eq!(v(" 1.2.3+build.7 "), v("1.2.3"));
        assert!(v("1.2.3-alpha.1").pre_release);
        for bad in [
            "", "v", "1..2", "1.2.3.4", "a.b.c", "1.2.x", "1.2.3-", "-1.2.3", "1.+2",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(v("0.1.10") > v("0.1.9"));
        assert!(v("0.2.0") > v("0.1.99"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(v("v0.1.3") == v("0.1.3"));
        assert!(v("0.1.4-alpha.1") < v("0.1.4"));
        assert!(v("0.1.4-alpha.1") > v("0.1.3"));
    }

    #[test]
    fn the_current_version_parses() {
        let current = Version::current();
        assert!(CURRENT_VERSION.starts_with(&current.to_string()));
    }

    const RELEASE_JSON: &str = r#"{
        "tag_name": "v0.1.4",
        "html_url": "https://github.com/pimgux/venecita/releases/tag/v0.1.4",
        "prerelease": false,
        "draft": false,
        "assets": [
            {"name": "venecita-dock-v0.1.4-macos.dmg",
             "browser_download_url": "https://github.com/pimgux/venecita/releases/download/v0.1.4/venecita-dock-v0.1.4-macos.dmg",
             "size": 4200000},
            {"name": "venecita-dock-v0.1.4-macos.dmg.sha256",
             "browser_download_url": "https://github.com/pimgux/venecita/releases/download/v0.1.4/venecita-dock-v0.1.4-macos.dmg.sha256",
             "size": 97},
            {"name": "venecita-dock-v0.1.4-windows-x86_64-setup.exe",
             "browser_download_url": "https://github.com/pimgux/venecita/releases/download/v0.1.4/venecita-dock-v0.1.4-windows-x86_64-setup.exe",
             "size": 3100000},
            {"name": "venecita-dock-v0.1.4-windows-x86_64-setup.exe.sha256",
             "browser_download_url": "https://github.com/pimgux/venecita/releases/download/v0.1.4/venecita-dock-v0.1.4-windows-x86_64-setup.exe.sha256"},
            {"name": "venecita-dock-v0.1.4-windows-x86_64.zip",
             "browser_download_url": "https://github.com/pimgux/venecita/releases/download/v0.1.4/venecita-dock-v0.1.4-windows-x86_64.zip"},
            {"name": "venecita-dock-v0.1.4-windows-x86_64.zip.sha256",
             "browser_download_url": "https://github.com/pimgux/venecita/releases/download/v0.1.4/venecita-dock-v0.1.4-windows-x86_64.zip.sha256"}
        ],
        "body": "notes"
    }"#;

    #[test]
    fn a_release_parses() {
        let release = parse_release(RELEASE_JSON).unwrap();
        assert_eq!(release.tag, "v0.1.4");
        assert_eq!(release.version, v("0.1.4"));
        assert_eq!(release.assets.len(), 6);
        assert_eq!(release.assets[0].size, Some(4_200_000));
        assert_eq!(release.assets[3].size, None);
        assert!(release.is_newer_than(v("0.1.3")));
        assert!(!release.is_newer_than(v("0.1.4")));
        assert!(!release.is_newer_than(v("0.2.0")));
        assert_eq!(release.safe_page_url(), release.page_url);
    }

    #[test]
    fn bad_or_pre_releases_are_rejected() {
        assert_eq!(parse_release("not json"), Err(UpdateError::BadResponse));
        assert_eq!(
            parse_release(r#"{"message": "Not Found"}"#),
            Err(UpdateError::BadResponse)
        );
        let pre = RELEASE_JSON.replace("\"prerelease\": false", "\"prerelease\": true");
        assert_eq!(parse_release(&pre), Err(UpdateError::BadResponse));
        let tag = RELEASE_JSON.replace("\"tag_name\": \"v0.1.4\"", "\"tag_name\": \"v0.1.4-rc.1\"");
        assert_eq!(parse_release(&tag), Err(UpdateError::BadResponse));
        let tag = RELEASE_JSON.replace("\"tag_name\": \"v0.1.4\"", "\"tag_name\": \"latest\"");
        assert_eq!(parse_release(&tag), Err(UpdateError::BadResponse));
    }

    #[test]
    fn an_odd_page_url_falls_back_to_the_releases_page() {
        let json = RELEASE_JSON.replace(
            "https://github.com/pimgux/venecita/releases/tag/v0.1.4",
            "https://evil.example/releases",
        );
        assert_eq!(
            parse_release(&json).unwrap().safe_page_url(),
            RELEASES_PAGE_URL
        );
    }

    #[test]
    fn the_package_for_each_system() {
        let release = parse_release(RELEASE_JSON).unwrap();
        let windows = select_package(&release, Os::Windows).unwrap();
        assert_eq!(
            windows.file.name,
            "venecita-dock-v0.1.4-windows-x86_64-setup.exe"
        );
        assert_eq!(
            windows.checksum.name,
            "venecita-dock-v0.1.4-windows-x86_64-setup.exe.sha256"
        );
        let mac = select_package(&release, Os::MacOs).unwrap();
        assert_eq!(mac.file.name, "venecita-dock-v0.1.4-macos.dmg");
        assert_eq!(mac.checksum.name, "venecita-dock-v0.1.4-macos.dmg.sha256");
        assert_eq!(select_package(&release, Os::Other), None);
    }

    #[test]
    fn no_package_without_its_checksum() {
        let mut release = parse_release(RELEASE_JSON).unwrap();
        release
            .assets
            .retain(|a| a.name != "venecita-dock-v0.1.4-windows-x86_64-setup.exe.sha256");
        assert_eq!(select_package(&release, Os::Windows), None);
        assert!(select_package(&release, Os::MacOs).is_some());
    }

    const HASH: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn checksum_files_parse() {
        let name = "venecita-dock-v0.1.4-macos.dmg";
        assert_eq!(parse_sha256(HASH, name).as_deref(), Some(HASH));
        assert_eq!(
            parse_sha256(&format!("{HASH}  {name}\n"), name).as_deref(),
            Some(HASH)
        );
        // Windows line endings, upper case, binary-mode marker.
        assert_eq!(
            parse_sha256(&format!("{}  *{name}\r\n", HASH.to_uppercase()), name).as_deref(),
            Some(HASH)
        );
        // A list of several files: the one for `name`.
        let other = "0".repeat(64);
        let list = format!("{other}  another.zip\r\n{HASH}  {name}\r\n");
        assert_eq!(parse_sha256(&list, name).as_deref(), Some(HASH));
    }

    #[test]
    fn bad_checksum_files_are_rejected() {
        let name = "venecita-dock-v0.1.4-macos.dmg";
        assert_eq!(parse_sha256("", name), None);
        assert_eq!(parse_sha256("abc123  file", name), None);
        assert_eq!(parse_sha256(&format!("{HASH}  other.dmg"), name), None);
        assert_eq!(parse_sha256(&format!("{}g", &HASH[..63]), name), None);
    }

    #[test]
    fn urls_are_redacted() {
        assert_eq!(
            redact_url("https://objects.githubusercontent.com/asset/1?X-Amz-Signature=abc#x"),
            "https://objects.githubusercontent.com/asset/1"
        );
        assert_eq!(
            redact_url("https://user:secret@evil.example/a"),
            "https://evil.example/a"
        );
        assert_eq!(redact_url("nonsense"), "(invalid URL)");
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
    }

    #[test]
    fn only_github_over_https() {
        for ok in [
            LATEST_RELEASE_URL,
            RELEASES_PAGE_URL,
            "https://github.com/pimgux/venecita/releases/download/v0.1.4/x.dmg",
            "https://objects.githubusercontent.com/github-production-release-asset/1?a=b",
            "https://release-assets.githubusercontent.com/github-production-release-asset/1",
            "https://GitHub.com/pimgux/venecita",
        ] {
            assert!(is_allowed_url(ok), "{ok}");
        }
        for bad in [
            "http://github.com/pimgux/venecita",
            "https://github.com.evil.example/x",
            "https://evil.example/github.com",
            "https://user:pass@github.com/x",
            "https://github.com:8443/x",
            "https://raw.githubusercontent.com/x",
            "file:///C:/Windows/System32/calc.exe",
            "github.com/pimgux/venecita",
            "",
        ] {
            assert!(!is_allowed_url(bad), "{bad}");
        }
    }
}
