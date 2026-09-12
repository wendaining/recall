mod app;
mod ui;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};

use crate::config::Config;
use crate::tui::runtime::TerminalGuard;
use app::App;

pub(crate) fn run(config: Config) -> Result<()> {
    let mut app = App::new(config);
    let mut guard = TerminalGuard::enter()?;
    let result = event_loop(guard.terminal(), &mut app);
    guard.leave();
    result
}

fn event_loop(
    terminal: &mut ratatui::Terminal<crate::tui::runtime::Backend>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key_for_size(app, key, crossterm::terminal::size()?);
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key_for_size(app: &mut App, key: crossterm::event::KeyEvent, size: (u16, u16)) {
    let exit_key = matches!(
        key.code,
        crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('q')
    ) || key
        .modifiers
        .contains(crossterm::event::KeyModifiers::CONTROL)
        && key.code == crossterm::event::KeyCode::Char('c');
    if size.0 < 72 || size.1 < 20 {
        app.should_quit |= exit_key;
    } else {
        app.handle_key(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn small_terminal_ignores_actions_but_allows_exit() {
        let mut app = App::new(crate::config::Config::default());
        app.category = 1;
        app.selected = 0;
        let original = app.config.proxy.secrets_filter;

        handle_key_for_size(
            &mut app,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            (60, 15),
        );
        assert_eq!(app.config.proxy.secrets_filter, original);
        assert!(!app.should_quit);

        handle_key_for_size(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            (60, 15),
        );
        assert!(app.should_quit);
    }
}
