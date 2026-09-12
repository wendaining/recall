mod app;
pub(crate) mod config;
mod runtime;
mod ui;

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::Terminal;

use crate::cli::SearchArgs;
use crate::config::Config;
use app::{Action, App};
use runtime::{Backend, TerminalGuard};

/// Launch the interactive TUI.
///
/// The interface is drawn to **stderr** so that stdout stays clean: any
/// selected command is printed to stdout after the terminal is restored, which
/// lets shell widgets capture it via `$(recall search --cmd-only)`.
///
/// Returns an exit code: 0 for a normal edit selection, 2 when the user asked
/// to rerun the command.
pub fn run(
    args: SearchArgs,
    config: Config,
    update_notice: Option<String>,
    update_result: Receiver<Option<String>>,
) -> Result<i32> {
    let mut app = App::new(config, args.cmd_only, args.query)?;
    if let Some(notice) = update_notice {
        app.set_status(notice);
    }

    let mut guard = TerminalGuard::enter()?;
    let result = event_loop(guard.terminal(), &mut app, update_result);
    guard.leave();

    result?;

    let mut code = 0;
    if let Some(command) = app.selected_command {
        println!("{command}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        if app.action == Action::Rerun {
            code = 2;
        }
    }
    Ok(code)
}

fn event_loop(
    terminal: &mut Terminal<Backend>,
    app: &mut App,
    update_result: Receiver<Option<String>>,
) -> Result<()> {
    let mut update_result = Some(update_result);
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key),
                _ => {}
            }
        }
        if let Some(receiver) = &update_result {
            match receiver.try_recv() {
                Ok(Some(notice)) => app.set_status(notice),
                Ok(None) | Err(TryRecvError::Disconnected) => update_result = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
