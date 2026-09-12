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
            app.handle_key(key);
        }
        if app.should_quit {
            return Ok(());
        }
    }
}
