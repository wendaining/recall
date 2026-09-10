use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;

/// Current time in nanoseconds since the Unix epoch.
pub fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Best-effort hostname without pulling in an extra dependency.
pub fn hostname() -> Option<String> {
    if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty())
}

/// Resolve the hostname, honoring a config override.
pub fn resolved_hostname(config: &Config) -> Option<String> {
    config.general.hostname.clone().or_else(hostname)
}

/// The user's login shell, falling back to /bin/sh.
pub fn login_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

/// Whether an executable is found on `PATH`.
pub fn command_exists(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(name);
        candidate.is_file()
    })
}

/// Strip ANSI escape sequences and normalize line endings.
pub fn strip_ansi(input: &[u8]) -> Vec<u8> {
    strip_ansi_escapes::strip(input)
}

/// Print a line to stderr, ignoring errors (used for diagnostics).
pub fn eprintln_flush(msg: &str) {
    let _ = writeln!(std::io::stderr(), "{msg}");
}

/// Render an absolute timestamp in local time using `format`.
pub fn format_time(ns: i64, format: &str) -> String {
    use chrono::{DateTime, Local};
    let dt = DateTime::from_timestamp_nanos(ns).with_timezone(&Local);
    dt.format(format).to_string()
}

/// Render a duration in nanoseconds in a compact human form.
pub fn format_duration(ns: i64) -> String {
    if ns < 0 {
        return "-".to_string();
    }
    let ms = ns as f64 / 1_000_000.0;
    if ms < 1000.0 {
        format!("{ms:.0}ms")
    } else if ms < 60_000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else {
        let secs = ms / 1000.0;
        format!("{}m{:.0}s", (secs / 60.0) as u64, secs % 60.0)
    }
}
