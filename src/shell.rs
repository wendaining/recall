use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
enum RcRoot {
    Home,
    ZdotdirOrHome,
    Config,
    Documents,
}

#[derive(Clone, Copy)]
struct KeyEncoding {
    alt_prefix: &'static str,
    ctrl_prefix: &'static str,
    ctrl_uppercase: bool,
    max_chords: usize,
}

pub(crate) struct Shell {
    pub(crate) name: &'static str,
    init_script: Option<&'static str>,
    rc_root: Option<RcRoot>,
    rc_tail: &'static [&'static str],
    key_encoding: Option<KeyEncoding>,
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
            ctrl_uppercase: true,
            max_chords: usize::MAX,
        }),
    },
    Shell {
        name: "bash",
        init_script: Some(include_str!("../shell/recall.bash")),
        rc_root: Some(RcRoot::Home),
        rc_tail: &[".bashrc"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "\\e",
            ctrl_prefix: "\\C-",
            ctrl_uppercase: false,
            max_chords: usize::MAX,
        }),
    },
    Shell {
        name: "fish",
        init_script: Some(include_str!("../shell/recall.fish")),
        rc_root: Some(RcRoot::Config),
        rc_tail: &["fish", "config.fish"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "\\e",
            ctrl_prefix: "\\c",
            ctrl_uppercase: false,
            max_chords: usize::MAX,
        }),
    },
    Shell {
        name: "pwsh",
        init_script: Some(include_str!("../shell/recall.ps1")),
        rc_root: Some(RcRoot::Documents),
        rc_tail: &["PowerShell", "Microsoft.PowerShell_profile.ps1"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "Alt+",
            ctrl_prefix: "Ctrl+",
            ctrl_uppercase: false,
            max_chords: 1,
        }),
    },
    Shell {
        name: "powershell",
        init_script: Some(include_str!("../shell/recall.ps1")),
        rc_root: Some(RcRoot::Documents),
        rc_tail: &["WindowsPowerShell", "Microsoft.PowerShell_profile.ps1"],
        key_encoding: Some(KeyEncoding {
            alt_prefix: "Alt+",
            ctrl_prefix: "Ctrl+",
            ctrl_uppercase: false,
            max_chords: 1,
        }),
    },
    Shell {
        name: "cmd",
        init_script: None,
        rc_root: None,
        rc_tail: &[],
        key_encoding: None,
    },
    Shell {
        name: "nu",
        init_script: None,
        rc_root: None,
        rc_tail: &[],
        key_encoding: None,
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
        for key in keys.iter().take(encoding.max_chords) {
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
            }
        }
        out
    }
}

pub(crate) fn parse_integration_name(value: &str) -> Result<String, String> {
    match Shell::from_name(value).filter(|shell| shell.init_script.is_some()) {
        Some(_) => Ok(value.to_string()),
        None => Err(format!("unsupported shell: {value}")),
    }
}

#[derive(Clone, Copy)]
struct RcDirs<'a> {
    home: Option<&'a Path>,
    zdotdir: Option<&'a Path>,
    config: Option<&'a Path>,
    documents: Option<&'a Path>,
}

enum Key {
    Alt(char),
    Ctrl(char),
}

fn parse_key_sequence(spec: &str) -> Option<Vec<Key>> {
    let mut keys = Vec::new();
    for token in spec.split_whitespace() {
        let (modifier, rest) = token.split_once('-')?;
        let mut chars = rest.chars();
        let ch = chars.next()?;
        if chars.next().is_some() || !ch.is_ascii_alphabetic() {
            return None;
        }
        keys.push(match modifier.to_ascii_lowercase().as_str() {
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
            "Ctrl+x"
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
}
