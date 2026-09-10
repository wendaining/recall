//! Best-effort secret detection for captured commands and output.
//!
//! The patterns mirror the defaults used by atuin. This is a safety net, not a
//! guarantee: it only prevents obviously sensitive values from being persisted.

use std::sync::OnceLock;

use regex::RegexSet;

fn patterns() -> &'static RegexSet {
    static SET: OnceLock<RegexSet> = OnceLock::new();
    SET.get_or_init(|| {
        RegexSet::new([
            r"AKIA[0-9A-Z]{16}",
            r"ghp_[0-9A-Za-z]{36}",
            r"github_pat_[0-9A-Za-z_]{82}",
            r"xox[baprs]-[0-9A-Za-z-]{10,}",
            r"sk_live_[0-9a-zA-Z]{24,}",
            r"rk_live_[0-9a-zA-Z]{24,}",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
        ])
        .expect("valid secret patterns")
    })
}

/// Whether `text` looks like it contains a known secret.
pub fn contains_secret(text: &str) -> bool {
    patterns().is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_aws_key() {
        assert!(contains_secret("export KEY=AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn ignores_normal_text() {
        assert!(!contains_secret("cargo build --release"));
    }
}
