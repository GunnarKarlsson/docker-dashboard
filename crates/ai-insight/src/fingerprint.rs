use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

/// Noise that changes between copies of the same bug (hex, paths, numbers).
static NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            0x[0-9a-fA-F]+
          | /[^\s]+
          | \b\d+\b
        ",
    )
    .expect("valid noise regex")
});

/// Secrets and real-world identifiers (MAC, Bearer, JWT-like blobs, email).
static SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            (?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}
          | Bearer\s+\S+
          | eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+
          | [A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}
        ",
    )
    .expect("valid secret regex")
});

const HOST_LABEL_HASH_LEN: usize = 8;
const FINGERPRINT_HEX_LEN: usize = 16;

/// Generates a stable, non-reversible host label from engine name and context id.
///
/// Format: `{sanitized_name}:{sha256(context_id) hex truncated to HOST_LABEL_HASH_LEN}`.
pub fn generate_host_label(name: &str, context_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(context_id.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    let name: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{name}:{}", &hex[..HOST_LABEL_HASH_LEN])
}

/// Generates a hex fingerprint of length `FINGERPRINT_HEX_LEN` from a container name and a noise-stripped message.
///
/// Hashes `tag` plus the message after replacing volatile noise (numbers, paths, hex) with `#`.
pub fn generate_fingerprint(tag: &str, message: &str) -> String {
    let collapsed = NOISE_RE.replace_all(message, "#");
    let mut hasher = Sha256::new();
    hasher.update(tag.as_bytes());
    hasher.update(b"|");
    hasher.update(collapsed.as_bytes());
    format!("{:x}", hasher.finalize())[..FINGERPRINT_HEX_LEN].to_string()
}

/// Generates a copy of `message` with secrets and real-world identifiers replaced by `#`.
pub fn redact(message: &str) -> String {
    SECRET_RE.replace_all(message, "#").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_numbers_and_paths() {
        let left = generate_fingerprint("worker", "failed 3 times at /var/lib/docker/foo");
        let right = generate_fingerprint("worker", "failed 9 times at /var/lib/docker/bar");
        assert_eq!(left, right);
    }

    #[test]
    fn different_tags_differ() {
        let left = generate_fingerprint("worker", "boom");
        let right = generate_fingerprint("api", "boom");
        assert_ne!(left, right);
    }

    #[test]
    fn redact_email_bearer_and_mac() {
        let text = redact("user a@b.com Bearer abc.def aa:bb:cc:dd:ee:ff");
        assert!(!text.contains("a@b.com"));
        assert!(!text.contains("abc.def"));
        assert!(!text.contains("aa:bb:cc:dd:ee:ff"));
        assert!(text.contains('#'));
    }

    #[test]
    fn redact_keeps_status_codes_and_paths() {
        let text = redact("HTTP 500 from /var/lib/docker/containers/abc");
        assert!(text.contains("500"));
        assert!(text.contains("/var/lib/docker/containers/abc"));
    }

    #[test]
    fn generate_host_label_hides_context_id() {
        let label = generate_host_label("Docker 28.3", "local");
        assert!(label.starts_with("Docker_28_3:"));
        assert!(!label.contains("local"));
        assert_eq!(label, generate_host_label("Docker 28.3", "local"));
        assert_ne!(label, generate_host_label("Docker 28.3", "ssh-host"));
    }
}
