use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{Config, ConfigStore};
use crate::shell::Shell;

const RECALL_COMMAND: &str = r"^\s*recall\b";
const PASSWORD_COMMANDS: &str = r"^\s*(?:pass|gopass|op|bw)\b";
const CONTAINER_LOGS: &str = r"^\s*(?:docker|kubectl)\s+logs\b";
const TAIL_FOLLOW: &str = r"^\s*tail\s+-f\b";
const FFMPEG: &str = r"^\s*ffmpeg\b";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Category {
    Keybinding,
    Capture,
    Appearance,
    Shell,
}

impl Category {
    pub(crate) const ALL: [Self; 4] = [
        Self::Keybinding,
        Self::Capture,
        Self::Appearance,
        Self::Shell,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Keybinding => "Keybinding",
            Self::Capture => "Capture rules",
            Self::Appearance => "Appearance",
            Self::Shell => "Shell integration",
        }
    }

    pub(crate) fn row_count(self) -> usize {
        match self {
            Self::Keybinding => 1,
            Self::Capture => 4,
            Self::Appearance => 3,
            Self::Shell => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuleTarget {
    Command,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Modal {
    KeyRecorder {
        chords: Vec<String>,
    },
    RuleEditor {
        target: RuleTarget,
        index: Option<usize>,
        input: String,
    },
    DeleteRule {
        target: RuleTarget,
        index: usize,
        pattern: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CaptureItem {
    Secrets,
    Interactive,
    Preset {
        label: &'static str,
        target: RuleTarget,
        pattern: &'static str,
    },
    Add(RuleTarget),
    Rule {
        target: RuleTarget,
        index: usize,
        pattern: String,
    },
}

pub(crate) struct App {
    pub(crate) config: Config,
    pub(crate) category: usize,
    pub(crate) selected: usize,
    pub(crate) should_quit: bool,
    pub(crate) show_help: bool,
    pub(crate) status: Option<String>,
    pub(crate) status_is_error: bool,
    pub(crate) modal: Option<Modal>,
    pub(crate) shell_name: String,
    store: ConfigStore,
}

impl App {
    pub(crate) fn new(config: Config) -> Self {
        let shell_name = crate::util::login_shell();
        Self {
            config,
            category: 0,
            selected: 0,
            should_quit: false,
            show_help: false,
            status: None,
            status_is_error: false,
            modal: None,
            shell_name,
            store: ConfigStore::active(),
        }
    }

    pub(crate) fn current_category(&self) -> Category {
        Category::ALL[self.category]
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        if self.modal.is_some() {
            self.handle_modal_key(key);
            return;
        }
        if key.code == KeyCode::F(1) {
            self.show_help = !self.show_help;
            return;
        }
        if self.show_help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                self.show_help = false;
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => self.move_category(1),
            KeyCode::BackTab => self.move_category(-1),
            KeyCode::Up => self.move_row(-1),
            KeyCode::Down => self.move_row(1),
            KeyCode::Char(' ') | KeyCode::Enter => self.activate_selected(),
            KeyCode::Char('a') if self.current_category() == Category::Capture => {
                self.add_rule_for_selection()
            }
            KeyCode::Char('e') if self.current_category() == Category::Capture => {
                self.edit_selected_rule()
            }
            KeyCode::Char('d') if self.current_category() == Category::Capture => {
                self.delete_selected_rule()
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    fn move_category(&mut self, delta: isize) {
        let len = Category::ALL.len() as isize;
        self.category = (self.category as isize + delta).rem_euclid(len) as usize;
        self.selected = 0;
        self.status = None;
    }

    fn move_row(&mut self, delta: isize) {
        let len = self.row_count() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    pub(crate) fn row_count(&self) -> usize {
        match self.current_category() {
            Category::Capture => self.capture_items().len(),
            category => category.row_count(),
        }
    }

    pub(crate) fn capture_items(&self) -> Vec<CaptureItem> {
        let presets = [
            CaptureItem::Preset {
                label: "Recall commands",
                target: RuleTarget::Command,
                pattern: RECALL_COMMAND,
            },
            CaptureItem::Preset {
                label: "Password managers",
                target: RuleTarget::Command,
                pattern: PASSWORD_COMMANDS,
            },
            CaptureItem::Preset {
                label: "Container logs output",
                target: RuleTarget::Output,
                pattern: CONTAINER_LOGS,
            },
            CaptureItem::Preset {
                label: "tail -f output",
                target: RuleTarget::Output,
                pattern: TAIL_FOLLOW,
            },
            CaptureItem::Preset {
                label: "ffmpeg output",
                target: RuleTarget::Output,
                pattern: FFMPEG,
            },
        ];
        let mut items = vec![CaptureItem::Secrets, CaptureItem::Interactive];
        items.extend(presets);
        items.push(CaptureItem::Add(RuleTarget::Command));
        items.extend(
            self.config
                .proxy
                .exclude
                .iter()
                .enumerate()
                .filter(|(_, pattern)| !is_preset(pattern))
                .map(|(index, pattern)| CaptureItem::Rule {
                    target: RuleTarget::Command,
                    index,
                    pattern: pattern.clone(),
                }),
        );
        items.push(CaptureItem::Add(RuleTarget::Output));
        items.extend(
            self.config
                .proxy
                .exclude_output
                .iter()
                .enumerate()
                .filter(|(_, pattern)| !is_preset(pattern))
                .map(|(index, pattern)| CaptureItem::Rule {
                    target: RuleTarget::Output,
                    index,
                    pattern: pattern.clone(),
                }),
        );
        items
    }

    pub(crate) fn preset_enabled(&self, target: RuleTarget, pattern: &str) -> bool {
        rules(&self.config, target)
            .iter()
            .any(|rule| rule == pattern)
    }

    fn activate_selected(&mut self) {
        if self.current_category() == Category::Keybinding {
            self.modal = Some(Modal::KeyRecorder { chords: Vec::new() });
            self.status = None;
            return;
        }
        if self.current_category() != Category::Capture {
            return;
        }
        let Some(item) = self.capture_items().get(self.selected).cloned() else {
            return;
        };
        match item {
            CaptureItem::Secrets => self.toggle_bool("secrets_filter"),
            CaptureItem::Interactive => self.toggle_bool("mark_interactive"),
            CaptureItem::Preset {
                target, pattern, ..
            } => self.toggle_preset(target, pattern),
            CaptureItem::Add(target) => self.open_rule_editor(target, None, String::new()),
            CaptureItem::Rule {
                target,
                index,
                pattern,
            } => self.open_rule_editor(target, Some(index), pattern),
        }
    }

    fn toggle_bool(&mut self, key: &str) {
        let next = match key {
            "secrets_filter" => !self.config.proxy.secrets_filter,
            "mark_interactive" => !self.config.proxy.mark_interactive,
            _ => return,
        };
        match self.store.set_bool("proxy", key, next) {
            Ok(config) => {
                self.config = config;
                self.set_saved(format!(
                    "{key} {}",
                    if next { "enabled" } else { "disabled" }
                ));
            }
            Err(err) => self.set_error(format!("failed to save {key}: {err}")),
        }
    }

    fn toggle_preset(&mut self, target: RuleTarget, pattern: &str) {
        let mut next = rules(&self.config, target).to_vec();
        if let Some(index) = next.iter().position(|rule| rule == pattern) {
            next.remove(index);
        } else {
            next.push(pattern.to_string());
        }
        self.save_rules(target, next, "capture preset updated");
    }

    fn add_rule_for_selection(&mut self) {
        let target = self
            .capture_items()
            .get(self.selected)
            .and_then(capture_target)
            .unwrap_or(RuleTarget::Command);
        self.open_rule_editor(target, None, String::new());
    }

    fn edit_selected_rule(&mut self) {
        let Some(CaptureItem::Rule {
            target,
            index,
            pattern,
        }) = self.capture_items().get(self.selected).cloned()
        else {
            self.set_error("select a custom rule to edit".to_string());
            return;
        };
        self.open_rule_editor(target, Some(index), pattern);
    }

    fn delete_selected_rule(&mut self) {
        let Some(CaptureItem::Rule {
            target,
            index,
            pattern,
        }) = self.capture_items().get(self.selected).cloned()
        else {
            self.set_error("select a custom rule to delete".to_string());
            return;
        };
        self.modal = Some(Modal::DeleteRule {
            target,
            index,
            pattern,
        });
    }

    fn open_rule_editor(&mut self, target: RuleTarget, index: Option<usize>, input: String) {
        self.modal = Some(Modal::RuleEditor {
            target,
            index,
            input,
        });
    }

    fn handle_modal_key(&mut self, key: KeyEvent) {
        let Some(mut modal) = self.modal.take() else {
            return;
        };
        match &mut modal {
            Modal::KeyRecorder { chords } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter if chords.is_empty() => {
                    self.set_error("press a Ctrl or Alt shortcut first".to_string());
                    self.modal = Some(modal);
                }
                KeyCode::Enter => {
                    let spec = chords.join(" ");
                    self.save_search_key(&spec);
                }
                KeyCode::Backspace => {
                    chords.pop();
                    self.modal = Some(modal);
                }
                _ => match semantic_chord(key) {
                    Some(chord) if chords.len() < 2 => {
                        chords.push(chord);
                        self.status = None;
                        self.modal = Some(modal);
                    }
                    Some(_) => {
                        self.set_error("a shortcut can contain at most two chords".to_string());
                        self.modal = Some(modal);
                    }
                    None => {
                        self.set_error(
                            "use Ctrl/Alt with a letter, or press Ctrl+Space".to_string(),
                        );
                        self.modal = Some(modal);
                    }
                },
            },
            Modal::RuleEditor {
                target,
                index,
                input,
            } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    let input = input.trim().to_string();
                    if input.is_empty() {
                        self.set_error("regular expression cannot be empty".to_string());
                        self.modal = Some(modal);
                    } else if let Err(err) = regex::Regex::new(&input) {
                        self.set_error(format!("invalid regular expression: {err}"));
                        self.modal = Some(modal);
                    } else {
                        let mut next = rules(&self.config, *target).to_vec();
                        match *index {
                            Some(index) if index < next.len() => next[index] = input,
                            Some(_) => self.set_error("rule no longer exists".to_string()),
                            None => next.push(input),
                        }
                        if index.is_none() || index.is_some_and(|index| index < next.len()) {
                            self.save_rules(*target, next, "capture rule saved");
                        }
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                    self.modal = Some(modal);
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    input.push(character);
                    self.modal = Some(modal);
                }
                _ => self.modal = Some(modal),
            },
            Modal::DeleteRule {
                target,
                index,
                pattern: _,
            } => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    let mut next = rules(&self.config, *target).to_vec();
                    if *index < next.len() {
                        next.remove(*index);
                        self.save_rules(*target, next, "capture rule deleted");
                        self.selected = self.selected.min(self.row_count().saturating_sub(1));
                    } else {
                        self.set_error("rule no longer exists".to_string());
                    }
                }
                KeyCode::Esc | KeyCode::Char('n') => {}
                _ => self.modal = Some(modal),
            },
        }
    }

    fn save_rules(&mut self, target: RuleTarget, rules: Vec<String>, status: &str) {
        let key = match target {
            RuleTarget::Command => "exclude",
            RuleTarget::Output => "exclude_output",
        };
        match self.store.set_strings("proxy", key, &rules) {
            Ok(config) => {
                self.config = config;
                self.set_saved(status.to_string());
            }
            Err(err) => self.set_error(format!("failed to save capture rules: {err}")),
        }
    }

    fn save_search_key(&mut self, spec: &str) {
        let Some(shell) = Shell::from_command(&self.shell_name) else {
            self.set_error(format!(
                "cannot validate shortcuts for unsupported shell {}",
                self.shell_name
            ));
            self.modal = Some(Modal::KeyRecorder {
                chords: spec.split_whitespace().map(str::to_string).collect(),
            });
            return;
        };
        if shell.semantic_search_key(spec).is_none() {
            self.set_error(format!("{spec} is not supported by {}", shell.name));
            self.modal = Some(Modal::KeyRecorder {
                chords: spec.split_whitespace().map(str::to_string).collect(),
            });
            return;
        }
        match self.store.set_string("ui", "search_key", spec) {
            Ok(config) => {
                self.config = config;
                self.set_saved("search shortcut saved; open a new shell to use it".to_string());
            }
            Err(err) => {
                self.set_error(format!("failed to save search shortcut: {err}"));
                self.modal = Some(Modal::KeyRecorder {
                    chords: spec.split_whitespace().map(str::to_string).collect(),
                });
            }
        }
    }

    fn set_saved(&mut self, status: String) {
        self.status = Some(status);
        self.status_is_error = false;
    }

    fn set_error(&mut self, status: String) {
        self.status = Some(status);
        self.status_is_error = true;
    }
}

fn semantic_chord(key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    if ctrl == alt
        || key
            .modifiers
            .intersects(KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META)
    {
        return None;
    }
    if ctrl && matches!(key.code, KeyCode::Null | KeyCode::Char(' ' | '@')) {
        return Some("ctrl-space".to_string());
    }
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    character.is_ascii_alphabetic().then(|| {
        format!(
            "{}-{}",
            if ctrl { "ctrl" } else { "alt" },
            character.to_ascii_lowercase()
        )
    })
}

fn rules(config: &Config, target: RuleTarget) -> &[String] {
    match target {
        RuleTarget::Command => &config.proxy.exclude,
        RuleTarget::Output => &config.proxy.exclude_output,
    }
}

fn is_preset(pattern: &str) -> bool {
    [
        RECALL_COMMAND,
        PASSWORD_COMMANDS,
        CONTAINER_LOGS,
        TAIL_FOLLOW,
        FFMPEG,
    ]
    .contains(&pattern)
}

fn capture_target(item: &CaptureItem) -> Option<RuleTarget> {
    match item {
        CaptureItem::Preset { target, .. }
        | CaptureItem::Add(target)
        | CaptureItem::Rule { target, .. } => Some(*target),
        CaptureItem::Secrets | CaptureItem::Interactive => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn persisted_app(name: &str) -> (App, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "recall-config-ui-test-{}-{}-{name}",
            std::process::id(),
            ulid::Ulid::generate()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let config = Config::default();
        std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();
        let mut app = App::new(config);
        app.shell_name = "zsh".to_string();
        app.store = ConfigStore::at(path.clone());
        (app, path, dir)
    }

    #[test]
    fn navigates_categories_and_rows() {
        let mut app = App::new(Config::default());
        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.current_category(), Category::Shell);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 2);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.current_category(), Category::Keybinding);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn help_closes_before_the_dashboard() {
        let mut app = App::new(Config::default());
        app.handle_key(key(KeyCode::F(1)));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.show_help);
        assert!(!app.should_quit);
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn capture_items_separate_presets_from_custom_rules() {
        let mut config = Config::default();
        config.proxy.exclude.push("^private".to_string());
        config.proxy.exclude_output.push(FFMPEG.to_string());
        config.proxy.exclude_output.push("^large".to_string());
        let app = App::new(config);
        let items = app.capture_items();

        assert!(items.iter().any(|item| matches!(
            item,
            CaptureItem::Rule { pattern, .. } if pattern == "^private"
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            CaptureItem::Rule { pattern, .. } if pattern == "^large"
        )));
        assert!(!items.iter().any(|item| matches!(
            item,
            CaptureItem::Rule { pattern, .. } if pattern == FFMPEG
        )));
    }

    #[test]
    fn invalid_rule_stays_in_editor() {
        let mut app = App::new(Config::default());
        app.modal = Some(Modal::RuleEditor {
            target: RuleTarget::Command,
            index: None,
            input: "[".to_string(),
        });
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.modal, Some(Modal::RuleEditor { .. })));
        assert!(app.status_is_error);
    }

    #[test]
    fn capture_changes_are_saved_immediately() {
        let (mut app, path, dir) = persisted_app("capture-save");
        app.category = 1;

        app.selected = 0;
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!Config::load_from(&path).unwrap().proxy.secrets_filter);

        app.selected = 3;
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(
            Config::load_from(&path)
                .unwrap()
                .proxy
                .exclude
                .contains(&PASSWORD_COMMANDS.to_string())
        );

        app.modal = Some(Modal::RuleEditor {
            target: RuleTarget::Output,
            index: None,
            input: "^large".to_string(),
        });
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            Config::load_from(&path).unwrap().proxy.exclude_output,
            ["^large"]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn records_two_chords_and_saves_semantic_key() {
        let (mut app, path, dir) = persisted_app("search-key");
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Null, KeyModifiers::CONTROL));
        app.handle_key(key(KeyCode::Enter));

        assert!(app.modal.is_none());
        assert_eq!(
            Config::load_from(&path).unwrap().ui.search_key,
            "ctrl-x ctrl-space"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_unmodified_and_third_chords() {
        let mut app = App::new(Config::default());
        app.modal = Some(Modal::KeyRecorder { chords: Vec::new() });
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.status_is_error);
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.status.as_deref().unwrap().contains("at most two"));
        assert!(matches!(
            app.modal,
            Some(Modal::KeyRecorder { ref chords }) if chords.len() == 2
        ));
    }
}
