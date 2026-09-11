use std::collections::HashSet;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::clipboard::Clipboard;
use crate::config::Config;
use crate::db::{Db, queries};
use crate::model::Block;

/// Maximum number of results loaded into the list.
const RESULT_LIMIT: usize = 2000;

/// Database key under which the user's preferred list pane width is stored.
const LIST_WIDTH_SETTING: &str = "ui.list_width_pct";
/// Percentage points the split moves per key press.
const LIST_WIDTH_STEP: u16 = 5;
/// Bounds keeping both panes usable.
const LIST_WIDTH_MIN: u16 = 15;
const LIST_WIDTH_MAX: u16 = 85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Search,
    Detail,
}

/// What the shell widget should do with the selected command after the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Insert the command into the prompt for editing.
    Edit,
    /// Execute the command immediately.
    Rerun,
}

pub struct App {
    pub db: Db,
    pub config: Config,
    pub clipboard: Box<dyn Clipboard>,
    pub query: String,
    pub results: Vec<Block>,
    pub selected: usize,
    pub selected_ids: HashSet<String>,
    pub list_state: ratatui::widgets::ListState,
    pub detail: Option<Block>,
    pub detail_scroll: u16,
    /// Width of the list pane (left) as a percentage of the window.
    pub list_width_pct: u16,
    pub focus: Focus,
    pub cmd_only: bool,
    /// Command to print after the TUI exits (selection / rerun).
    pub selected_command: Option<String>,
    pub action: Action,
    pub status: Option<String>,
    pub status_is_error: bool,
    pub should_quit: bool,
    pub show_help: bool,
}

impl App {
    pub fn new(config: Config, cmd_only: bool, initial_query: Option<String>) -> Result<Self> {
        let db = Db::open(&config.general.db_path)?;
        if config.retention.auto_prune && config.retention.retention_days > 0 {
            let _ = queries::prune(
                &db.conn,
                config.retention.retention_days,
                crate::util::now_ns(),
            );
        }
        let clipboard = crate::clipboard::ClipboardChain::detect(&config.clipboard);
        let list_width_pct = queries::get_setting(&db.conn, LIST_WIDTH_SETTING)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(config.ui.list_width_pct);
        let mut app = Self {
            db,
            config,
            clipboard: Box::new(clipboard),
            query: initial_query.unwrap_or_default(),
            results: Vec::new(),
            selected: 0,
            selected_ids: HashSet::new(),
            list_state: ratatui::widgets::ListState::default(),
            detail: None,
            detail_scroll: 0,
            list_width_pct: clamp_list_width(list_width_pct),
            focus: Focus::Search,
            cmd_only,
            selected_command: None,
            action: Action::Edit,
            status: None,
            status_is_error: false,
            should_quit: false,
            show_help: false,
        };
        app.refresh();
        Ok(app)
    }

    pub fn refresh(&mut self) {
        match queries::search(&self.db.conn, &self.query, RESULT_LIMIT) {
            Ok(results) => {
                self.results = results;
                if self.selected >= self.results.len() {
                    self.selected = self.results.len().saturating_sub(1);
                }
                self.load_detail();
            }
            Err(err) => self.set_error(format!("search failed: {err}")),
        }
    }

    pub fn load_detail(&mut self) {
        self.detail_scroll = 0;
        let id = self.results.get(self.selected).map(|b| b.id.clone());
        self.detail = match id {
            Some(id) => queries::get(&self.db.conn, &id).ok().flatten(),
            None => None,
        };
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.results.is_empty() {
            return;
        }
        let len = self.results.len() as isize;
        let mut next = self.selected as isize + delta;
        if next < 0 {
            next = 0;
        }
        if next >= len {
            next = len - 1;
        }
        if next as usize != self.selected {
            self.selected = next as usize;
            self.load_detail();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Global shortcuts.
        match key.code {
            KeyCode::Left => {
                self.resize_list(-(LIST_WIDTH_STEP as i16));
                return;
            }
            KeyCode::Right => {
                self.resize_list(LIST_WIDTH_STEP as i16);
                return;
            }
            KeyCode::Char('c') if ctrl => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('y') if ctrl => {
                self.copy_command();
                return;
            }
            KeyCode::Char('t') if ctrl => {
                self.toggle_selected();
                return;
            }
            KeyCode::Char('o') if ctrl => {
                if self.selected_ids.is_empty() {
                    self.copy_output();
                } else {
                    self.copy_selected_transcript();
                }
                return;
            }
            KeyCode::Char('e') if ctrl => {
                self.dispatch(Action::Rerun);
                return;
            }
            KeyCode::F(1) => {
                self.show_help = !self.show_help;
                return;
            }
            KeyCode::PageUp => {
                self.scroll_detail(-10);
                return;
            }
            KeyCode::PageDown => {
                self.scroll_detail(10);
                return;
            }
            KeyCode::Enter if ctrl => {
                self.dispatch(Action::Rerun);
                return;
            }
            KeyCode::Enter => {
                self.focus = match self.focus {
                    Focus::Search => Focus::Detail,
                    Focus::Detail => Focus::Search,
                };
                return;
            }
            KeyCode::Tab => {
                self.dispatch(Action::Edit);
                return;
            }
            _ => {}
        }

        match self.focus {
            Focus::Search => self.handle_search_key(key, ctrl),
            Focus::Detail => self.handle_detail_key(key),
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent, ctrl: bool) {
        match key.code {
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.refresh();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refresh();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Esc => {
                if self.query.is_empty() {
                    self.should_quit = true;
                } else {
                    self.query.clear();
                    self.refresh();
                }
            }
            _ => {}
        }
    }

    fn handle_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.scroll_detail(-1),
            KeyCode::Down => self.scroll_detail(1),
            KeyCode::Home => self.detail_scroll = 0,
            KeyCode::End => self.detail_scroll = u16::MAX,
            KeyCode::Char('y') => self.copy_command(),
            KeyCode::Char('Y') => self.copy_output(),
            KeyCode::Esc => self.focus = Focus::Search,
            _ => {}
        }
    }

    /// Move the split and persist the new width for future runs.
    fn resize_list(&mut self, delta: i16) {
        let next = (self.list_width_pct as i16 + delta)
            .clamp(LIST_WIDTH_MIN as i16, LIST_WIDTH_MAX as i16) as u16;
        if next == self.list_width_pct {
            return;
        }
        self.list_width_pct = next;
        match queries::set_setting(&self.db.conn, LIST_WIDTH_SETTING, &next.to_string()) {
            Ok(()) => self.set_status(format!("list width {next}%")),
            Err(err) => self.set_error(format!("failed to save layout: {err}")),
        }
    }

    fn scroll_detail(&mut self, delta: i32) {
        let next = self.detail_scroll as i32 + delta;
        self.detail_scroll = next.max(0) as u16;
    }

    fn select(&mut self, action: Action) {
        if let Some(block) = self.results.get(self.selected) {
            self.selected_command = Some(block.command.clone());
            self.action = action;
            self.should_quit = true;
        }
    }

    /// In `--cmd-only` mode (the shell widget) the selection is printed for the
    /// shell to act on; standalone it is copied to the clipboard.
    fn dispatch(&mut self, action: Action) {
        if self.cmd_only {
            self.select(action);
        } else {
            self.copy_command();
        }
    }

    fn copy_command(&mut self) {
        if let Some(block) = self.results.get(self.selected) {
            let command = block.command.clone();
            self.copy_text(&command, "command");
        }
    }

    fn copy_output(&mut self) {
        let output = self
            .detail
            .as_ref()
            .and_then(|block| block.output.as_ref())
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
        match output {
            Some(text) if !text.is_empty() => self.copy_text(&text, "output"),
            _ => self.set_error("no output to copy".to_string()),
        }
    }

    fn toggle_selected(&mut self) {
        let Some(id) = self
            .results
            .get(self.selected)
            .map(|block| block.id.clone())
        else {
            return;
        };
        if !self.selected_ids.remove(&id) {
            self.selected_ids.insert(id);
        }
        self.set_status(format!("{} blocks selected", self.selected_ids.len()));
    }

    fn copy_selected_transcript(&mut self) {
        match self.selected_blocks() {
            Ok(blocks) if !blocks.is_empty() => {
                let count = blocks.len();
                let transcript = format_transcript(&blocks);
                self.copy_text(&transcript, &format!("{count} blocks"));
            }
            Ok(_) => self.set_error("no selected blocks to copy".to_string()),
            Err(err) => self.set_error(format!("loading selected blocks failed: {err}")),
        }
    }

    fn selected_blocks(&self) -> Result<Vec<Block>> {
        let mut blocks = self
            .selected_ids
            .iter()
            .map(|id| queries::get(&self.db.conn, id))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        blocks.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(blocks)
    }

    fn copy_text(&mut self, text: &str, label: &str) {
        let backend = self.clipboard.name();
        match self.clipboard.copy(text) {
            Ok(()) => self.set_status(format!("copied {label} via {backend}")),
            Err(err) => self.set_error(format!("clipboard error: {err}")),
        }
    }

    pub fn set_status(&mut self, message: String) {
        self.status = Some(message);
        self.status_is_error = false;
    }

    pub fn set_error(&mut self, message: String) {
        self.status = Some(message);
        self.status_is_error = true;
    }
}

fn clamp_list_width(pct: u16) -> u16 {
    pct.clamp(LIST_WIDTH_MIN, LIST_WIDTH_MAX)
}

fn format_transcript(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|block| {
            let mut section = format!("$ {}", block.command);
            if let Some(output) = block.output.as_deref().filter(|output| !output.is_empty()) {
                section.push('\n');
                section.push_str(&String::from_utf8_lossy(output));
            }
            section
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::model::BlockKind;

    struct TestClipboard {
        copied: Arc<Mutex<Option<String>>>,
    }

    impl Clipboard for TestClipboard {
        fn copy(&self, text: &str) -> Result<()> {
            *self.copied.lock().unwrap() = Some(text.to_string());
            Ok(())
        }

        fn name(&self) -> &'static str {
            "test"
        }
    }

    fn block(id: &str, command: &str, output: &str, started_at: i64) -> Block {
        Block {
            id: id.to_string(),
            command: command.to_string(),
            started_at,
            output: Some(output.as_bytes().to_vec()),
            output_bytes: output.len() as i64,
            output_lines: output.lines().count() as i64,
            kind: BlockKind::Normal,
            created_at: started_at,
            ..Block::default()
        }
    }

    fn test_app(blocks: &[Block]) -> (App, Arc<Mutex<Option<String>>>) {
        let db = Db::open_in_memory().unwrap();
        for block in blocks {
            queries::insert(&db.conn, block).unwrap();
        }
        let copied = Arc::new(Mutex::new(None));
        let clipboard = TestClipboard {
            copied: copied.clone(),
        };
        let mut app = App {
            db,
            config: Config::default(),
            clipboard: Box::new(clipboard),
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            selected_ids: HashSet::new(),
            list_state: ratatui::widgets::ListState::default(),
            detail: None,
            detail_scroll: 0,
            list_width_pct: 42,
            focus: Focus::Search,
            cmd_only: false,
            selected_command: None,
            action: Action::Edit,
            status: None,
            status_is_error: false,
            should_quit: false,
            show_help: false,
        };
        app.refresh();
        (app, copied)
    }

    #[test]
    fn ctrl_o_copies_selected_blocks_in_chronological_order() {
        let blocks = [
            block("old", "first command", "first output", 100),
            block("new", "second command", "second output", 200),
        ];
        let (mut app, copied) = test_app(&blocks);

        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        app.move_selection(1);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));

        assert_eq!(
            copied.lock().unwrap().as_deref(),
            Some("$ first command\nfirst output\n\n$ second command\nsecond output")
        );
    }

    #[test]
    fn arrows_resize_and_persist_list_width() {
        let (mut app, _) = test_app(&[block("one", "command", "output", 100)]);

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.list_width_pct, 47);
        assert_eq!(
            queries::get_setting(&app.db.conn, LIST_WIDTH_SETTING)
                .unwrap()
                .as_deref(),
            Some("47")
        );

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(app.list_width_pct, 42);
    }

    #[test]
    fn list_width_is_clamped() {
        let (mut app, _) = test_app(&[]);
        app.list_width_pct = LIST_WIDTH_MIN;

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(app.list_width_pct, LIST_WIDTH_MIN);

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.list_width_pct, LIST_WIDTH_MIN + LIST_WIDTH_STEP);
    }

    #[test]
    fn q_does_not_leave_detail_or_quit() {
        let (mut app, _) = test_app(&[block("one", "command", "output", 100)]);
        app.focus = Focus::Detail;

        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

        assert_eq!(app.focus, Focus::Detail);
        assert!(!app.should_quit);
    }
}
