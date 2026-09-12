use std::io;

use anyhow::Result;
#[cfg(not(windows))]
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub(crate) type Backend = CrosstermBackend<io::Stderr>;

/// Owns the terminal state and restores it on drop, so a panic never leaves the
/// user's terminal in raw mode.
pub(crate) struct TerminalGuard {
    terminal: Terminal<Backend>,
}

impl TerminalGuard {
    pub(crate) fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stderr = io::stderr();
        if let Err(err) = execute!(stderr, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }
        #[cfg(not(windows))]
        let _ = execute!(
            stderr,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
        let terminal = match Terminal::new(CrosstermBackend::new(stderr)) {
            Ok(terminal) => terminal,
            Err(err) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stderr(), LeaveAlternateScreen);
                return Err(err.into());
            }
        };
        Ok(Self { terminal })
    }

    pub(crate) fn terminal(&mut self) -> &mut Terminal<Backend> {
        &mut self.terminal
    }

    pub(crate) fn leave(&mut self) {
        let _ = disable_raw_mode();
        #[cfg(not(windows))]
        let _ = execute!(
            self.terminal.backend_mut(),
            PopKeyboardEnhancementFlags,
            LeaveAlternateScreen
        );
        #[cfg(windows)]
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.leave();
    }
}
