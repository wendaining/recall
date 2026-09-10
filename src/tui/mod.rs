mod app;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::cli::SearchArgs;
use crate::config::Config;
use app::{Action, App};

type Backend = CrosstermBackend<io::Stderr>;

/// Launch the interactive TUI.
///
/// The interface is drawn to **stderr** so that stdout stays clean: any
/// selected command is printed to stdout after the terminal is restored, which
/// lets shell widgets capture it via `$(recall search --cmd-only)`.
///
/// Returns an exit code: 0 for a normal edit selection, 2 when the user asked
/// to rerun the command.
pub fn run(args: SearchArgs, config: Config) -> Result<i32> {
    let mut app = App::new(config, args.cmd_only, args.query)?;

    let mut guard = TerminalGuard::enter()?;
    let result = event_loop(guard.terminal(), &mut app);
    guard.leave();

    result?;

    let mut code = 0;
    if let Some(command) = app.selected_command {
        println!("{command}");
        use std::io::Write;
        let _ = io::stdout().flush();
        if app.action == Action::Rerun {
            code = 2;
        }
    }
    Ok(code)
}

fn event_loop(terminal: &mut Terminal<Backend>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key),
                _ => {}
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Owns the terminal state and restores it on drop, so a panic never leaves the
/// user's terminal in raw mode.
struct TerminalGuard {
    terminal: Terminal<Backend>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stderr = io::stderr();
        execute!(stderr, EnterAlternateScreen)?;
        // Ask the terminal (kitty et al.) to report modifier keys such as
        // Ctrl+Enter distinctly. Ignored on terminals that don't support it.
        let _ = execute!(
            stderr,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
        let terminal = Terminal::new(CrosstermBackend::new(stderr))?;
        Ok(Self { terminal })
    }

    fn terminal(&mut self) -> &mut Terminal<Backend> {
        &mut self.terminal
    }

    fn leave(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            PopKeyboardEnhancementFlags,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.leave();
    }
}
