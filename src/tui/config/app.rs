use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Config;

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

pub(crate) struct App {
    pub(crate) config: Config,
    pub(crate) category: usize,
    pub(crate) selected: usize,
    pub(crate) should_quit: bool,
    pub(crate) show_help: bool,
    pub(crate) status: Option<String>,
    pub(crate) status_is_error: bool,
}

impl App {
    pub(crate) fn new(config: Config) -> Self {
        Self {
            config,
            category: 0,
            selected: 0,
            should_quit: false,
            show_help: false,
            status: None,
            status_is_error: false,
        }
    }

    pub(crate) fn current_category(&self) -> Category {
        Category::ALL[self.category]
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
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
        let len = self.current_category().row_count() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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
}
