use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Top-level configuration, loaded from `~/.config/recall/config.toml`.
///
/// Every field has a sensible default so the file is entirely optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub proxy: Proxy,
    pub retention: Retention,
    pub clipboard: ClipboardConfig,
    pub ui: Ui,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// Path to the recall SQLite database.
    pub db_path: PathBuf,
    /// Path to atuin's history database (used for import / linking).
    pub atuin_db_path: PathBuf,
    /// Maximum uncompressed bytes stored per command output.
    pub max_output_bytes: usize,
    /// Strip ANSI escape sequences before storing output.
    pub strip_ansi: bool,
    /// Hostname override; defaults to the system hostname.
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Proxy {
    /// Shell to spawn under the proxy. Empty means the login shell.
    pub shell: String,
    /// Spawn the shell as a login shell (`-l`). Defaults to true on macOS.
    pub login_shell: bool,
    /// Commands matching any of these regexes are not recorded.
    pub exclude: Vec<String>,
    /// Commands matching any of these regexes are recorded without output.
    pub exclude_output: Vec<String>,
    /// Mark alt-screen programs as `interactive` and skip their output.
    pub mark_interactive: bool,
    /// Enable the secrets filter.
    pub secrets_filter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Retention {
    /// Drop stored output older than this many days. 0 disables expiry.
    pub retention_days: u32,
    /// Run `prune` automatically on TUI startup.
    pub auto_prune: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClipboardConfig {
    /// One of: auto, arboard, osc52, wl-copy, xclip, xsel.
    pub backend: String,
    /// Maximum number of base64 bytes to send over OSC 52.
    pub max_osc52_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// Key that opens the recall search TUI. Semantic form, e.g. "alt-r",
    /// "ctrl-t", or a two-stroke sequence like "ctrl-x ctrl-r".
    pub search_key: String,
    /// Number of output lines shown per block in the list pane.
    pub preview_lines: usize,
    /// Initial width of the list pane as a percentage of the window. The TUI
    /// lets the user adjust this interactively; adjustments are persisted in
    /// the database and override this default on later runs.
    pub list_width_pct: u16,
    /// strftime format used for absolute timestamps.
    pub date_format: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            db_path: default_data_dir().join("recall.db"),
            atuin_db_path: default_atuin_db_path(),
            max_output_bytes: 1024 * 1024,
            strip_ansi: true,
            hostname: None,
        }
    }
}

impl Default for Proxy {
    fn default() -> Self {
        Self {
            shell: String::new(),
            login_shell: cfg!(target_os = "macos"),
            exclude: vec![r"^\s*recall\b".to_string(), r"^\s*atuin\b".to_string()],
            exclude_output: Vec::new(),
            mark_interactive: true,
            secrets_filter: true,
        }
    }
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            retention_days: 30,
            auto_prune: true,
        }
    }
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            backend: "auto".to_string(),
            max_osc52_bytes: 74_994,
        }
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            search_key: "alt-r".to_string(),
            preview_lines: 4,
            list_width_pct: 42,
            date_format: "%Y-%m-%d %H:%M:%S".to_string(),
        }
    }
}

impl Config {
    /// Path of the config file.
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("recall")
            .join("config.toml")
    }

    /// Load configuration from the default path, falling back to defaults if
    /// the file does not exist. `RECALL_CONFIG` overrides the path.
    pub fn load() -> Result<Self> {
        let path = std::env::var_os("RECALL_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(Self::config_path);
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
    }
}

pub fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recall")
}

pub fn default_atuin_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("atuin")
        .join("history.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exclude_output_patterns() {
        let config: Config = toml::from_str(
            r#"
            [proxy]
            exclude_output = ["^docker logs", "^tail -f"]
            "#,
        )
        .unwrap();

        assert_eq!(
            config.proxy.exclude_output,
            ["^docker logs".to_string(), "^tail -f".to_string()]
        );
    }

    #[test]
    fn login_shell_default_matches_platform() {
        assert_eq!(Proxy::default().login_shell, cfg!(target_os = "macos"));
    }

    #[test]
    fn default_list_width() {
        assert_eq!(Ui::default().list_width_pct, 42);
        let config: Config = toml::from_str(
            r#"
            [ui]
            list_width_pct = 60
            "#,
        )
        .unwrap();
        assert_eq!(config.ui.list_width_pct, 60);
    }

    #[test]
    fn parses_search_key() {
        let config: Config = toml::from_str(
            r#"
            [ui]
            search_key = "ctrl-x ctrl-r"
            "#,
        )
        .unwrap();

        assert_eq!(config.ui.search_key, "ctrl-x ctrl-r");
        assert_eq!(Config::default().ui.search_key, "alt-r");
    }
}
