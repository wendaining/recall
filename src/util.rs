use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;

/// Current time in nanoseconds since the Unix epoch.
pub fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Best-effort hostname, cross-platform.
pub fn hostname() -> Option<String> {
    let name = gethostname::gethostname();
    let name = name.to_string_lossy().trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

/// Resolve the hostname, honoring a config override.
pub fn resolved_hostname(config: &Config) -> Option<String> {
    config.general.hostname.clone().or_else(hostname)
}

/// The user's login shell.
#[cfg(unix)]
pub fn login_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

/// The user's shell, detected from the current process tree when possible so
/// that `recall shell` starts the same shell the user is already in.
#[cfg(windows)]
pub fn login_shell() -> String {
    detect_shell_from_parent()
        .or_else(env_shell)
        .unwrap_or_else(default_windows_shell)
}

/// Walk up the parent processes (skipping our own `recall` wrappers) looking for
/// a known shell.
#[cfg(windows)]
fn detect_shell_from_parent() -> Option<String> {
    use sysinfo::{ProcessesToUpdate, System};

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let mut pid = sysinfo::get_current_pid().ok()?;

    for _ in 0..8 {
        let parent = system.process(pid)?.parent()?;
        let process = system.process(parent)?;
        let name = process.name().to_string_lossy();
        let stem = name.trim_end_matches(".exe").to_ascii_lowercase();
        if stem == "recall" {
            pid = parent;
            continue;
        }
        let shell = known_shell(&stem)?;
        return Some(shell_command(shell, process.exe()));
    }
    None
}

/// Map a lowercased process image stem to a spawnable shell name.
#[cfg(windows)]
fn known_shell(stem: &str) -> Option<&'static str> {
    match stem {
        "pwsh" => Some("pwsh"),
        "powershell" => Some("powershell"),
        "cmd" => Some("cmd"),
        "bash" => Some("bash"),
        "zsh" => Some("zsh"),
        "fish" => Some("fish"),
        "nu" => Some("nu"),
        _ => None,
    }
}

/// Prefer the parent's full executable path (needed for Git Bash/MSYS shells),
/// falling back to the plain name for shells resolved from `PATH`.
#[cfg(windows)]
fn shell_command(name: &str, exe: Option<&Path>) -> String {
    match exe {
        Some(path) if path.is_file() => path.to_string_lossy().into_owned(),
        _ => name.to_string(),
    }
}

/// `$SHELL` if it points at an existing absolute Windows path (Git Bash, MSYS).
#[cfg(windows)]
fn env_shell() -> Option<String> {
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty())?;
    let path = Path::new(&shell);
    (path.is_absolute() && path.is_file()).then_some(shell)
}

#[cfg(windows)]
fn default_windows_shell() -> String {
    if command_exists("pwsh") {
        "pwsh".to_string()
    } else if command_exists("powershell") {
        "powershell".to_string()
    } else {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
}

/// Startup file where `recall init` should be added for the given shell.
pub fn shell_rc_path(shell: &str) -> Option<PathBuf> {
    let name = Path::new(shell)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(shell);
    let home = dirs::home_dir();
    let zdotdir = std::env::var_os("ZDOTDIR").map(PathBuf::from);
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".config")));
    rc_path_for(
        name,
        home.as_deref(),
        zdotdir.as_deref(),
        config_dir.as_deref(),
        dirs::document_dir().as_deref(),
    )
}

fn rc_path_for(
    shell: &str,
    home: Option<&Path>,
    zdotdir: Option<&Path>,
    config_dir: Option<&Path>,
    document_dir: Option<&Path>,
) -> Option<PathBuf> {
    match shell {
        "zsh" => Some(zdotdir.or(home)?.join(".zshrc")),
        "bash" => Some(home?.join(".bashrc")),
        "fish" => Some(config_dir?.join("fish").join("config.fish")),
        "pwsh" => Some(
            document_dir?
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
        "powershell" => Some(
            document_dir?
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
        _ => None,
    }
}

/// Whether an executable is found on `PATH`.
pub fn command_exists(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        if dir.join(name).is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            for ext in ["exe", "cmd", "bat", "ps1"] {
                if dir.join(format!("{name}.{ext}")).is_file() {
                    return true;
                }
            }
        }
        false
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_rc_paths_per_shell() {
        let home = Path::new("/home/u");
        let config = Path::new("/home/u/.config");
        let documents = Path::new("C:/Users/u/Documents");

        assert_eq!(
            rc_path_for("zsh", Some(home), None, Some(config), None).unwrap(),
            home.join(".zshrc")
        );
        assert_eq!(
            rc_path_for(
                "zsh",
                Some(home),
                Some(Path::new("/zdot")),
                Some(config),
                None
            )
            .unwrap(),
            Path::new("/zdot/.zshrc")
        );
        assert_eq!(
            rc_path_for("bash", Some(home), None, Some(config), None).unwrap(),
            home.join(".bashrc")
        );
        assert_eq!(
            rc_path_for("fish", Some(home), None, Some(config), None).unwrap(),
            config.join("fish").join("config.fish")
        );
        assert_eq!(
            rc_path_for("pwsh", None, None, None, Some(documents)).unwrap(),
            documents
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1")
        );
        assert_eq!(
            rc_path_for("powershell", None, None, None, Some(documents)).unwrap(),
            documents
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1")
        );
        assert!(rc_path_for("nu", Some(home), None, Some(config), None).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn maps_known_shell_process_names() {
        assert_eq!(known_shell("pwsh"), Some("pwsh"));
        assert_eq!(known_shell("powershell"), Some("powershell"));
        assert_eq!(known_shell("cmd"), Some("cmd"));
        assert_eq!(known_shell("bash"), Some("bash"));
        assert_eq!(known_shell("explorer"), None);
        assert_eq!(known_shell("recall"), None);
    }
}
