use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use portable_pty::{CommandBuilder, MasterPty, PtySize};

use crate::util;

pub(crate) struct RawModeGuard {
    active: bool,
    original_input_mode: Option<u32>,
}

impl RawModeGuard {
    pub(crate) fn new() -> Self {
        let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        let original_input_mode = if is_tty {
            enable_virtual_terminal_input()
        } else {
            None
        };
        Self {
            active: is_tty && crossterm::terminal::enable_raw_mode().is_ok(),
            original_input_mode,
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        if let Some(mode) = self.original_input_mode {
            restore_console_mode(mode);
        }
    }
}

pub(crate) fn configure_shell(_cmd: &mut CommandBuilder, _login: bool) {}

pub(crate) fn spawn_resize_handler(
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    running: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut last = crossterm::terminal::size().unwrap_or((0, 0));
        while running.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
            let Ok(size) = crossterm::terminal::size() else {
                continue;
            };
            if size != last {
                last = size;
                resize_pty(&master);
            }
        }
    });
}

fn resize_pty(master: &Arc<Mutex<Box<dyn MasterPty + Send>>>) {
    if let Some(size) = terminal_size() {
        let master = master.lock().unwrap();
        let _ = master.resize(size);
    }
}

pub(crate) fn terminal_size() -> Option<PtySize> {
    crossterm::terminal::size()
        .ok()
        .map(|(cols, rows)| PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
}

fn enable_virtual_terminal_input() -> Option<u32> {
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE,
        SetConsoleMode,
    };

    // SAFETY: reads and writes the console mode of this process's stdin handle.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode = 0u32;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return None;
        }
        let original = mode;
        if SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_INPUT) == 0 {
            return None;
        }
        if std::env::var_os("RECALL_DEBUG").is_some() {
            util::eprintln_flush(&format!(
                "recall[debug]: VT input enabled (was {original:#x})"
            ));
        }
        Some(original)
    }
}

fn restore_console_mode(mode: u32) {
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE, SetConsoleMode};

    // SAFETY: restores the mode captured from this process's stdin handle.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        SetConsoleMode(handle, mode);
    }
}
