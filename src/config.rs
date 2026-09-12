use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::atomic_file;

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
            exclude: vec![r"^\s*recall\b".to_string()],
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
    /// Path of the active config file. `RECALL_CONFIG` overrides the default.
    pub fn config_path() -> PathBuf {
        config_path_from(std::env::var_os("RECALL_CONFIG"))
    }

    /// Load configuration from the default path, falling back to defaults if
    /// the file does not exist. `RECALL_CONFIG` overrides the path.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing config {}", path.display()))
    }

    fn parse(text: &str) -> Result<Self> {
        let mut value: toml::Value = toml::from_str(text)?;
        // Configs generated before standalone imports contained this field.
        // Accept it during upgrades without retaining it in the current model.
        if let Some(general) = value.get_mut("general").and_then(toml::Value::as_table_mut) {
            general.remove("atuin_db_path");
        }
        Ok(value.try_into()?)
    }
}

/// Lossless, atomic mutations of the active TOML configuration.
#[allow(dead_code)]
pub(crate) struct ConfigStore {
    path: PathBuf,
}

#[allow(dead_code)]
impl ConfigStore {
    pub(crate) fn active() -> Self {
        Self {
            path: Config::config_path(),
        }
    }

    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn set_bool(&self, section: &str, key: &str, value: bool) -> Result<Config> {
        self.set_value(section, key, Value::from(value))
    }

    pub(crate) fn set_integer(&self, section: &str, key: &str, value: i64) -> Result<Config> {
        self.set_value(section, key, Value::from(value))
    }

    pub(crate) fn set_string(&self, section: &str, key: &str, value: &str) -> Result<Config> {
        self.set_value(section, key, Value::from(value))
    }

    pub(crate) fn set_strings(
        &self,
        section: &str,
        key: &str,
        values: &[String],
    ) -> Result<Config> {
        let mut array = Array::new();
        for value in values {
            array.push(value.as_str());
        }
        self.set_value(section, key, Value::Array(array))
    }

    fn set_value(&self, section: &str, key: &str, value: Value) -> Result<Config> {
        let original = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                toml::to_string_pretty(&Config::default())?.into_bytes()
            }
            Err(err) => {
                return Err(err).with_context(|| format!("reading config {}", self.path.display()));
            }
        };
        let (bom, bytes) = original
            .strip_prefix(&[0xef, 0xbb, 0xbf])
            .map_or((false, original.as_slice()), |bytes| (true, bytes));
        let text = std::str::from_utf8(bytes)
            .with_context(|| format!("decoding config {} as UTF-8", self.path.display()))?;
        let crlf = text.contains("\r\n");
        let normalized = if crlf {
            text.replace("\r\n", "\n")
        } else {
            text.to_string()
        };
        let mut document = normalized
            .parse::<DocumentMut>()
            .with_context(|| format!("parsing config {}", self.path.display()))?;
        let item = document
            .entry(section)
            .or_insert_with(|| Item::Table(Table::new()));
        let table = item
            .as_table_mut()
            .ok_or_else(|| anyhow!("config section `{section}` is not a table"))?;
        table[key] = Item::Value(value);

        let rendered = document.to_string();
        let config = Config::parse(&rendered)
            .with_context(|| format!("validating config {}", self.path.display()))?;
        let rendered = if crlf {
            rendered.replace('\n', "\r\n")
        } else {
            rendered
        };
        let mut output = Vec::with_capacity(rendered.len() + usize::from(bom) * 3);
        if bom {
            output.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        output.extend_from_slice(rendered.as_bytes());
        atomic_file::replace(&self.path, &output, "config")?;
        Ok(config)
    }
}

fn config_path_from(override_path: Option<std::ffi::OsString>) -> PathBuf {
    override_path.map(PathBuf::from).unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("recall")
            .join("config.toml")
    })
}

pub fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recall")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "recall-config-test-{}-{}-{name}",
            std::process::id(),
            ulid::Ulid::generate()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (dir.join("config.toml"), dir)
    }

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

    #[test]
    fn accepts_removed_import_path_from_generated_configs() {
        let config = Config::parse(
            r#"
            [general]
            atuin_db_path = "/old/history.db"
            max_output_bytes = 2048
            "#,
        )
        .unwrap();

        assert_eq!(config.general.max_output_bytes, 2048);
        assert!(!toml::to_string(&config).unwrap().contains("atuin_db_path"));
    }

    #[test]
    fn config_path_prefers_override() {
        let path = config_path_from(Some(std::ffi::OsString::from("custom/config.toml")));
        assert_eq!(path, PathBuf::from("custom/config.toml"));
    }

    #[test]
    fn config_store_preserves_comments_and_external_changes() {
        let (path, dir) = temp_config("comments");
        std::fs::write(
            &path,
            "# keep me\n[ui]\nsearch_key = \"alt-r\" # inline\n\n[proxy]\nshell = \"zsh\"\n",
        )
        .unwrap();
        let store = ConfigStore::at(path.clone());

        store.set_integer("ui", "preview_lines", 7).unwrap();
        let mut external = std::fs::read_to_string(&path).unwrap();
        external.push_str("# added elsewhere\n");
        std::fs::write(&path, external).unwrap();
        store.set_bool("proxy", "secrets_filter", false).unwrap();

        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# keep me"));
        assert!(saved.contains("search_key = \"alt-r\" # inline"));
        assert!(saved.contains("# added elsewhere"));
        assert!(saved.contains("preview_lines = 7"));
        assert!(saved.contains("secrets_filter = false"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_store_preserves_crlf_and_bom() {
        let (path, dir) = temp_config("crlf-bom");
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(b"[ui]\r\nsearch_key = \"alt-r\"\r\n");
        std::fs::write(&path, bytes).unwrap();

        ConfigStore::at(path.clone())
            .set_string("ui", "search_key", "ctrl-t")
            .unwrap();

        let saved = std::fs::read(&path).unwrap();
        assert!(saved.starts_with(&[0xef, 0xbb, 0xbf]));
        let text = std::str::from_utf8(&saved[3..]).unwrap();
        assert!(text.contains("search_key = \"ctrl-t\"\r\n"));
        assert!(!text.replace("\r\n", "").contains('\n'));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_store_refuses_invalid_toml_without_modifying_it() {
        let (path, dir) = temp_config("invalid");
        let original = b"[ui\ninvalid";
        std::fs::write(&path, original).unwrap();

        assert!(
            ConfigStore::at(path.clone())
                .set_bool("proxy", "secrets_filter", false)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_store_writes_string_arrays() {
        let (path, dir) = temp_config("arrays");
        let store = ConfigStore::at(path.clone());
        let config = store
            .set_strings(
                "proxy",
                "exclude_output",
                &["^docker logs".to_string(), "^tail -f".to_string()],
            )
            .unwrap();
        assert_eq!(config.proxy.exclude_output, ["^docker logs", "^tail -f"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
