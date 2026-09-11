use std::io::IsTerminal;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{CommandBuilder, MasterPty, PtySize};

pub(crate) struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    pub(crate) fn new() -> Self {
        let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        Self {
            active: is_tty && crossterm::terminal::enable_raw_mode().is_ok(),
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

pub(crate) fn configure_shell(cmd: &mut CommandBuilder, login: bool) {
    if login {
        cmd.arg("-l");
    }
}

pub(crate) fn spawn_resize_handler(
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    _running: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        use signal_hook::consts::SIGWINCH;
        use signal_hook::iterator::Signals;

        let Ok(mut signals) = Signals::new([SIGWINCH]) else {
            return;
        };
        for _ in signals.forever() {
            resize_pty(&master);
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
    let mut fallback = None;
    for fd in [libc::STDOUT_FILENO, libc::STDIN_FILENO] {
        let Some(size) = terminal_size_for_fd(fd) else {
            continue;
        };
        if size.pixel_width != 0 || size.pixel_height != 0 {
            return Some(size);
        }
        fallback = Some(size);
    }
    fallback.or_else(crossterm_terminal_size)
}

fn terminal_size_for_fd(fd: libc::c_int) -> Option<PtySize> {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::uninit();
    // SAFETY: `size` points to writable storage for `winsize`; the file
    // descriptor remains valid for the duration of this call.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, size.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: a successful TIOCGWINSZ call initialized the full value.
    let size = unsafe { size.assume_init() };
    if size.ws_row == 0 || size.ws_col == 0 {
        return None;
    }
    Some(PtySize {
        rows: size.ws_row,
        cols: size.ws_col,
        pixel_width: size.ws_xpixel,
        pixel_height: size.ws_ypixel,
    })
}

fn crossterm_terminal_size() -> Option<PtySize> {
    crossterm::terminal::size()
        .ok()
        .map(|(cols, rows)| PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::fd::{AsRawFd, FromRawFd};

    use super::*;

    #[test]
    fn reads_cell_and_pixel_dimensions_from_pty() {
        let mut expected = libc::winsize {
            ws_row: 42,
            ws_col: 132,
            ws_xpixel: 1584,
            ws_ypixel: 840,
        };
        let mut master_fd = -1;
        let mut slave_fd = -1;
        let expected_ptr = std::ptr::from_mut(&mut expected);
        // SAFETY: all output pointers are valid, and `expected` is initialized.
        let result = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                expected_ptr,
            )
        };
        assert_eq!(result, 0);

        // SAFETY: openpty returned these owned descriptors, each converted once.
        let (_master, slave) =
            unsafe { (File::from_raw_fd(master_fd), File::from_raw_fd(slave_fd)) };
        let actual = terminal_size_for_fd(slave.as_raw_fd()).unwrap();

        assert_eq!(actual.rows, expected.ws_row);
        assert_eq!(actual.cols, expected.ws_col);
        assert_eq!(actual.pixel_width, expected.ws_xpixel);
        assert_eq!(actual.pixel_height, expected.ws_ypixel);
    }
}
