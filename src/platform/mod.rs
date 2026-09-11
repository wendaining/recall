use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

#[cfg(unix)]
#[path = "unix.rs"]
mod imp;
#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

pub(crate) use imp::{RawModeGuard, configure_shell, spawn_resize_handler, terminal_size};

pub(crate) fn spawn_input_forwarder(
    mut writer: Box<dyn Write + Send>,
    input_bytes: Arc<AtomicUsize>,
) {
    thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 8192];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    input_bytes.fetch_add(n, Ordering::SeqCst);
                    if writer.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = writer.flush();
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });
}
