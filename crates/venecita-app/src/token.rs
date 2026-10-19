//! The local API access token (`token.secret`, next to `config.toml`).
//!
//! The token is 32 random bytes from the OS CSPRNG, encoded as unpadded base64url, and the
//! file is created with mode 0600. The settings window shows, copies and rotates it; the API
//! server (phase 8) checks requests against it.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// File name of the token, inside the config directory.
pub const TOKEN_FILE: &str = "token.secret";

/// Where the token lives for a given configuration file.
pub fn path_for(config_file: &Path) -> PathBuf {
    config_file.with_file_name(TOKEN_FILE)
}

/// Reads the token, if the file exists and is not empty.
pub fn read(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let token = text.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// Generates a new token.
pub fn generate() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(base64url(&bytes))
}

/// Replaces the token with a new one and returns it. The file is written with mode 0600.
pub fn rotate(path: &Path) -> std::io::Result<String> {
    let token = generate()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write to a sibling file first so a crash never leaves a truncated token behind.
    let tmp = path.with_extension("secret.tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(token.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(token)
}

/// Unpadded base64url (RFC 4648 §5).
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        let chars = chunk.len() + 1; // 1 byte → 2 chars, 2 → 3, 3 → 4
        for i in 0..chars {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 0x3f) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_matches_the_rfc_vectors() {
        // RFC 4648 §10 test vectors, without padding.
        for (input, expected) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
            ("fooba", "Zm9vYmE"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64url(input.as_bytes()), expected);
        }
        // The URL-safe alphabet uses `-` and `_` instead of `+` and `/`.
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn tokens_are_long_random_and_url_safe() {
        let a = generate().unwrap();
        let b = generate().unwrap();
        assert_eq!(a.len(), 43, "32 bytes → 43 base64url chars");
        assert_ne!(a, b);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn rotate_writes_a_private_file() {
        let dir = std::env::temp_dir().join(format!("venecita-token-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = path_for(&dir.join("config.toml"));
        assert_eq!(read(&path), None);

        let first = rotate(&path).unwrap();
        assert_eq!(read(&path).as_deref(), Some(first.as_str()));
        let second = rotate(&path).unwrap();
        assert_ne!(first, second);
        assert_eq!(read(&path).as_deref(), Some(second.as_str()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
