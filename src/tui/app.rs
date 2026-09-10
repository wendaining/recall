use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::clipboard::Clipboard;
use crate::config::Config;
use crate::db::{Db, queries};
use crate::model::Block;

/// Maximum number of results loaded into the list.
const RESULT_LIMIT: usize = 2000;

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
    pub list_state: ratatui::widgets::ListState,
    pub detail: Option<Block>,
    pub detail_scroll: u16,
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
        let clipboard = crate::clipboard::ClipboardChain::detect(&config.clipboard);
        let mut app = Self {
            db,
            config,
            clipboard: Box::new(clipboard),
            query: initial_query.unwrap_or_default(),
            results: Vec::new(),
            selected: 0,
            list_state: ratatui::widgets::ListState::default(),
            detail: None,
            detail_scroll: 0,
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
            KeyCode::Char('c') if ctrl => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('y') if ctrl => {
                self.copy_command();
                return;
            }
            KeyCode::Char('o') if ctrl => {
                self.copy_output();
                return;
            }
            KeyCode::Char('r') if ctrl => {
                self.select(Action::Rerun);
                return;
            }
            KeyCode::F(1) => {
                self.show_help = !self.show_help;
                return;
            }
            KeyCode::Up => {
                self.move_selection(-1);
                return;
            }
            KeyCode::Down => {
                self.move_selection(1);
                return;
            }
            KeyCode::Char('p') if ctrl => {
                self.move_selection(-1);
                return;
            }
            KeyCode::Char('n') if ctrl => {
                self.move_selection(1);
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
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Search => Focus::Detail,
                    Focus::Detail => Focus::Search,
                };
                return;
            }
            KeyCode::Enter => {
                self.accept();
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
            KeyCode::Char('j') => self.scroll_detail(1),
            KeyCode::Char('k') => self.scroll_detail(-1),
            KeyCode::Char('g') | KeyCode::Home => self.detail_scroll = 0,
            KeyCode::Char('G') | KeyCode::End => self.detail_scroll = u16::MAX,
            KeyCode::Char('y') => self.copy_command(),
            KeyCode::Char('Y') => self.copy_output(),
            KeyCode::Char('r') => self.select(Action::Rerun),
            KeyCode::Char('q') | KeyCode::Esc => self.focus = Focus::Search,
            _ => {}
        }
    }

    fn scroll_detail(&mut self, delta: i32) {
        let next = self.detail_scroll as i32 + delta;
        self.detail_scroll = next.max(0) as u16;
    }

    fn accept(&mut self) {
        if self.cmd_only {
            self.select(Action::Edit);
        } else {
            self.copy_command();
        }
    }

    fn select(&mut self, action: Action) {
        if let Some(block) = self.results.get(self.selected) {
            self.selected_command = Some(block.command.clone());
            self.action = action;
            self.should_quit = true;
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
