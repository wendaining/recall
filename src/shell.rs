use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
enum RcRoot {
    Home,
    ZdotdirOrHome,
    Config,
    Documents,
}

#[derive(Clone, Copy)]
enum HistoryRoot {
    Home,
    Data,
    PowerShellData,
}

#[derive(Clone, Copy)]
pub(crate) enum HistoryFormat {
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

#[derive(Clone, Copy)]
struct KeyEncoding {
    alt_prefix: &'static str,
    ctrl_prefix: &'static str,
    ctrl_space: &'static str,
    ctrl_uppercase: bool,
    max_chords: usize,
    chord_separator: &'static str,
}

pub(crate) struct Shell {
    pub(crate) name: &'static str,
    init_script: Option<&'static str>,
    rc_root: Option<RcRoot>,
    rc_tail: &'static [&'static str],
    key_encoding: Option<KeyEncoding>,
    history_root: Option<HistoryRoot>,
    history_tail: &'static [&'static str],
    history_format: Option<HistoryFormat>,
}

static SHELLS: &[Shell] = &[
    Shell {
        name: "zsh",
        init_script: Some(include_str!("../shell/recall.zsh")),
        rc_root: Some(RcRoot::ZdotdirOrHome),
        rc_tail: &[".zshrc"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "^[",
            ctrl_prefix: "^",
            ctrl_space: "^@",
            ctrl_uppercase: true,
            max_chords: usize::MAX,
            chord_separator: "",
        }),
        history_root: Some(HistoryRoot::Home),
        history_tail: &[".zsh_history"],
        history_format: Some(HistoryFormat::Zsh),
    },
    Shell {
        name: "bash",
        init_script: Some(include_str!("../shell/recall.bash")),
        rc_root: Some(RcRoot::Home),
        rc_tail: &[".bashrc"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "\\e",
            ctrl_prefix: "\\C-",
            ctrl_space: "\\C-@",
            ctrl_uppercase: false,
            max_chords: usize::MAX,
            chord_separator: "",
        }),
        history_root: Some(HistoryRoot::Home),
        history_tail: &[".bash_history"],
        history_format: Some(HistoryFormat::Bash),
    },
    Shell {
        name: "fish",
        init_script: Some(include_str!("../shell/recall.fish")),
        rc_root: Some(RcRoot::Config),
        rc_tail: &["fish", "config.fish"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "\\e",
            ctrl_prefix: "\\c",
            ctrl_space: "\\c@",
            ctrl_uppercase: false,
            max_chords: usize::MAX,
            chord_separator: "",
        }),
        history_root: Some(HistoryRoot::Data),
        history_tail: &["fish", "fish_history"],
        history_format: Some(HistoryFormat::Fish),
    },
    Shell {
        name: "pwsh",
        init_script: Some(include_str!("../shell/recall.ps1")),
        rc_root: Some(RcRoot::Documents),
        rc_tail: &["PowerShell", "Microsoft.PowerShell_profile.ps1"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "Alt+",
            ctrl_prefix: "Ctrl+",
            ctrl_space: "Ctrl+Spacebar",
            ctrl_uppercase: false,
            max_chords: 2,
            chord_separator: ",",
        }),
        history_root: Some(HistoryRoot::PowerShellData),
        history_tail: &["PSReadLine", "ConsoleHost_history.txt"],
        history_format: Some(HistoryFormat::PowerShell),
    },
    Shell {
        name: "powershell",
        init_script: Some(include_str!("../shell/recall.ps1")),
        rc_root: Some(RcRoot::Documents),
        rc_tail: &["WindowsPowerShell", "Microsoft.PowerShell_profile.ps1"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "Alt+",
            ctrl_prefix: "Ctrl+",
            ctrl_space: "Ctrl+Spacebar",
            ctrl_uppercase: false,
            max_chords: 2,
            chord_separator: ",",
        }),
        history_root: Some(HistoryRoot::PowerShellData),
        history_tail: &["PSReadLine", "ConsoleHost_history.txt"],
        history_format: Some(HistoryFormat::PowerShell),
    },
    Shell {
        name: "cmd",
        init_script: None,
        rc_root: None,
        rc_tail: &[],
        key_encoding: None,
        history_root: None,
        history_tail: &[],
        history_format: None,
    },
    Shell {
        name: "nu",
        init_script: None,
        rc_root: None,
        rc_tail: &[],
        key_encoding: None,
        history_root: None,
        history_tail: &[],
        history_format: None,
    },
];

impl Shell {
    pub(crate) fn from_name(name: &str) -> Option<&'static Self> {
        SHELLS.iter().find(|shell| shell.name == name)
    }

    pub(crate) fn from_command(command: &str) -> Option<&'static Self> {
        let name = Path::new(command)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(command)
            .to_ascii_lowercase();
        Self::from_name(&name)
    }

    pub(crate) fn init_script(&self) -> Option<&'static str> {
        self.init_script
    }

    pub(crate) fn rc_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir();
        let zdotdir = std::env::var_os("ZDOTDIR").map(PathBuf::from);
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|home| home.join(".config")));
        self.rc_path_from(&RcDirs {
            home: home.as_deref(),
            zdotdir: zdotdir.as_deref(),
            config: config.as_deref(),
            documents: dirs::document_dir().as_deref(),
        })
    }

    fn rc_path_from(&self, dirs: &RcDirs<'_>) -> Option<PathBuf> {
        let mut path = match self.rc_root? {
            RcRoot::Home => dirs.home?.to_path_buf(),
            RcRoot::ZdotdirOrHome => dirs.zdotdir.or(dirs.home)?.to_path_buf(),
            RcRoot::Config => dirs.config?.to_path_buf(),
            RcRoot::Documents => dirs.documents?.to_path_buf(),
        };
        path.extend(self.rc_tail);
        Some(path)
    }

    pub(crate) fn search_key(&self, spec: &str) -> String {
        let Some(keys) = parse_key_sequence(spec) else {
            return spec.to_string();
        };
        let Some(encoding) = self.key_encoding else {
            return spec.to_string();
        };

        let mut out = String::new();
        for (index, key) in keys.iter().take(encoding.max_chords).enumerate() {
            if index > 0 {
                out.push_str(encoding.chord_separator);
            }
            match key {
                Key::Alt(ch) => {
                    out.push_str(encoding.alt_prefix);
                    out.push(ch.to_ascii_lowercase());
                }
                Key::Ctrl(ch) => {
                    out.push_str(encoding.ctrl_prefix);
                    out.push(if encoding.ctrl_uppercase {
                        ch.to_ascii_uppercase()
                    } else {
                        ch.to_ascii_lowercase()
                    });
                }
                Key::CtrlSpace => out.push_str(encoding.ctrl_space),
            }
        }
        out
    }

    pub(crate) fn semantic_search_key(&self, spec: &str) -> Option<String> {
        let keys = parse_key_sequence(spec)?;
        let encoding = self.key_encoding?;
        (keys.len() <= encoding.max_chords).then(|| self.search_key(spec))
    }

    pub(crate) fn history_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir();
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.as_ref().map(|home| home.join(".local").join("share")));
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        self.history_path_from(&HistoryDirs {
            home: home.as_deref(),
            data: data.as_deref(),
            appdata: appdata.as_deref(),
        })
    }

    fn history_path_from(&self, dirs: &HistoryDirs<'_>) -> Option<PathBuf> {
        let mut path = match self.history_root? {
            HistoryRoot::Home => dirs.home?.to_path_buf(),
            HistoryRoot::Data => dirs.data?.to_path_buf(),
            HistoryRoot::PowerShellData if cfg!(windows) => dirs
                .appdata?
                .join("Microsoft")
                .join("Windows")
                .join("PowerShell"),
            HistoryRoot::PowerShellData => dirs.data?.join("powershell"),
        };
        path.extend(self.history_tail);
        Some(path)
    }

    pub(crate) fn history_format(&self) -> Option<HistoryFormat> {
        self.history_format
    }
}

pub(crate) fn parse_integration_name(value: &str) -> Result<String, String> {
    match Shell::from_name(value).filter(|shell| shell.init_script.is_some()) {
        Some(_) => Ok(value.to_string()),
        None => Err(format!("unsupported shell: {value}")),
    }
}

pub(crate) fn parse_history_name(value: &str) -> Result<String, String> {
    match Shell::from_name(value).filter(|shell| shell.history_format.is_some()) {
        Some(_) => Ok(value.to_string()),
        None => Err(format!("unsupported history format: {value}")),
    }
}

#[derive(Clone, Copy)]
struct RcDirs<'a> {
    home: Option<&'a Path>,
    zdotdir: Option<&'a Path>,
    config: Option<&'a Path>,
    documents: Option<&'a Path>,
}

struct HistoryDirs<'a> {
    home: Option<&'a Path>,
    data: Option<&'a Path>,
    appdata: Option<&'a Path>,
}

enum Key {
    Alt(char),
    Ctrl(char),
    CtrlSpace,
}

fn parse_key_sequence(spec: &str) -> Option<Vec<Key>> {
    let mut keys = Vec::new();
    for token in spec.split_whitespace() {
        let (modifier, rest) = token.split_once('-')?;
        let modifier = modifier.to_ascii_lowercase();
        if matches!(modifier.as_str(), "ctrl" | "control") && rest.eq_ignore_ascii_case("space") {
            keys.push(Key::CtrlSpace);
            continue;
        }
        let mut chars = rest.chars();
        let ch = chars.next()?;
        if chars.next().is_some() || !ch.is_ascii_alphabetic() {
            return None;
        }
        keys.push(match modifier.as_str() {
            "alt" | "meta" | "option" => Key::Alt(ch),
            "ctrl" | "control" => Key::Ctrl(ch),
            _ => return None,
        });
    }
    (!keys.is_empty()).then_some(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_keys_from_shell_table() {
        assert_eq!(Shell::from_name("zsh").unwrap().search_key("alt-r"), "^[r");
        assert_eq!(
            Shell::from_name("bash").unwrap().search_key("alt-r"),
            "\\er"
        );
        assert_eq!(
            Shell::from_name("fish").unwrap().search_key("alt-r"),
            "\\er"
        );
        assert_eq!(Shell::from_name("zsh").unwrap().search_key("ctrl-t"), "^T");
        assert_eq!(
            Shell::from_name("bash").unwrap().search_key("ctrl-t"),
            "\\C-t"
        );
        assert_eq!(
            Shell::from_name("fish").unwrap().search_key("ctrl-t"),
            "\\ct"
        );
        assert_eq!(
            Shell::from_name("pwsh")
                .unwrap()
                .search_key("ctrl-x ctrl-r"),
            "Ctrl+x,Ctrl+r"
        );
        assert_eq!(
            Shell::from_name("zsh").unwrap().search_key("ctrl-x ctrl-r"),
            "^X^R"
        );
        assert_eq!(
            Shell::from_name("bash")
                .unwrap()
                .search_key("ctrl-x ctrl-r"),
            "\\C-x\\C-r"
        );
        assert_eq!(
            Shell::from_name("fish")
                .unwrap()
                .search_key("ctrl-x ctrl-r"),
            "\\cx\\cr"
        );
        assert_eq!(
            Shell::from_name("powershell").unwrap().search_key("alt-r"),
            "Alt+r"
        );
        assert_eq!(
            Shell::from_name("zsh").unwrap().search_key("ctrl-space"),
            "^@"
        );
        assert_eq!(
            Shell::from_name("bash").unwrap().search_key("ctrl-space"),
            "\\C-@"
        );
        assert_eq!(
            Shell::from_name("fish").unwrap().search_key("ctrl-space"),
            "\\c@"
        );
        assert_eq!(
            Shell::from_name("pwsh").unwrap().search_key("ctrl-space"),
            "Ctrl+Spacebar"
        );
    }

    #[test]
    fn resolves_rc_paths_from_shell_table() {
        let home = Path::new("/home/u");
        let config = Path::new("/home/u/.config");
        let documents = Path::new("C:/Users/u/Documents");
        let dirs = RcDirs {
            home: Some(home),
            zdotdir: None,
            config: Some(config),
            documents: Some(documents),
        };

        assert_eq!(
            Shell::from_name("zsh").unwrap().rc_path_from(&dirs),
            Some(home.join(".zshrc"))
        );
        let zdot_dirs = RcDirs {
            zdotdir: Some(Path::new("/zdot")),
            ..dirs
        };
        assert_eq!(
            Shell::from_name("zsh").unwrap().rc_path_from(&zdot_dirs),
            Some(PathBuf::from("/zdot/.zshrc"))
        );
        assert_eq!(
            Shell::from_name("bash").unwrap().rc_path_from(&dirs),
            Some(home.join(".bashrc"))
        );
        assert_eq!(
            Shell::from_name("fish").unwrap().rc_path_from(&dirs),
            Some(config.join("fish").join("config.fish"))
        );
        assert_eq!(
            Shell::from_name("pwsh").unwrap().rc_path_from(&dirs),
            Some(
                documents
                    .join("PowerShell")
                    .join("Microsoft.PowerShell_profile.ps1")
            )
        );
        assert_eq!(
            Shell::from_name("powershell").unwrap().rc_path_from(&dirs),
            Some(
                documents
                    .join("WindowsPowerShell")
                    .join("Microsoft.PowerShell_profile.ps1")
            )
        );
        assert_eq!(Shell::from_name("nu").unwrap().rc_path_from(&dirs), None);
    }

    #[test]
    fn accepts_aliases_and_passes_through_unknown_specs() {
        let zsh = Shell::from_name("zsh").unwrap();
        assert_eq!(zsh.search_key("Option-R"), "^[r");
        assert_eq!(zsh.search_key("META-r"), "^[r");
        assert_eq!(zsh.search_key("not-a-key"), "not-a-key");
        assert_eq!(zsh.search_key(""), "");
    }

    #[test]
    fn recognizes_executable_paths() {
        assert_eq!(
            Shell::from_command("/bin/zsh").map(|shell| shell.name),
            Some("zsh")
        );
        assert_eq!(
            Shell::from_command("pwsh.exe").map(|shell| shell.name),
            Some("pwsh")
        );
        assert!(Shell::from_command("unknown").is_none());
    }

    #[test]
    fn resolves_default_history_paths() {
        let dirs = HistoryDirs {
            home: Some(Path::new("/home/u")),
            data: Some(Path::new("/home/u/.local/share")),
            appdata: Some(Path::new("C:/Users/u/AppData/Roaming")),
        };

        assert_eq!(
            Shell::from_name("zsh").unwrap().history_path_from(&dirs),
            Some(PathBuf::from("/home/u/.zsh_history"))
        );
        assert_eq!(
            Shell::from_name("fish").unwrap().history_path_from(&dirs),
            Some(PathBuf::from("/home/u/.local/share/fish/fish_history"))
        );
        let powershell = Shell::from_name("pwsh")
            .unwrap()
            .history_path_from(&dirs)
            .unwrap();
        if cfg!(windows) {
            assert_eq!(
                powershell,
                PathBuf::from(
                    "C:/Users/u/AppData/Roaming/Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt"
                )
            );
        } else {
            assert_eq!(
                powershell,
                PathBuf::from("/home/u/.local/share/powershell/PSReadLine/ConsoleHost_history.txt")
            );
        }
    }
}
