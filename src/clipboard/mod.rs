//! Cross-platform clipboard abstraction.
//!
//! Backends are deliberately pluggable so no single implementation is required:
//! native (arboard), OSC 52 escape sequences (works over SSH/tmux/kitty), and
//! external commands (wl-copy on Wayland, xclip/xsel on X11).

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};
use base64::Engine;

use crate::config::ClipboardConfig;
use crate::util;

/// A clipboard implementation.
pub trait Clipboard: Send + Sync {
    /// Copy `text` to the system clipboard.
    fn copy(&self, text: &str) -> Result<()>;
    /// Human-readable backend name.
    fn name(&self) -> &'static str;
}

/// Copy via an external command, feeding the text on stdin.
struct ExternalClipboard {
    name: &'static str,
    program: &'static str,
    args: &'static [&'static str],
}

impl Clipboard for ExternalClipboard {
    fn copy(&self, text: &str) -> Result<()> {
        let mut child = Command::new(self.program)
            .args(self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            bail!("{} exited with {}", self.program, status);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// Copy via the OSC 52 terminal escape sequence. Works in kitty, tmux and SSH.
struct Osc52Clipboard {
    max_bytes: usize,
}

impl Clipboard for Osc52Clipboard {
    fn copy(&self, text: &str) -> Result<()> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
        if encoded.len() > self.max_bytes {
            bail!(
                "text too large for OSC 52 ({} > {} bytes)",
                encoded.len(),
                self.max_bytes
            );
        }
        let mut stdout = std::io::stdout();
        write!(stdout, "\x1b]52;c;{encoded}\x07")?;
        stdout.flush()?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "osc52"
    }
}

/// Native clipboard via arboard (Wayland, X11, macOS, Windows).
struct ArboardClipboard;

impl Clipboard for ArboardClipboard {
    fn copy(&self, text: &str) -> Result<()> {
        let mut clipboard = arboard::Clipboard::new()?;
        clipboard.set_text(text.to_string())?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "arboard"
    }
}

/// Try several backends in order, using the first that succeeds.
pub struct ClipboardChain {
    backends: Vec<Box<dyn Clipboard>>,
}

impl Clipboard for ClipboardChain {
    fn copy(&self, text: &str) -> Result<()> {
        let mut last_error = None;
        for backend in &self.backends {
            match backend.copy(text) {
                Ok(()) => return Ok(()),
                Err(err) => last_error = Some(err),
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no clipboard backend available")))
    }

    fn name(&self) -> &'static str {
        self.backends.first().map(|b| b.name()).unwrap_or("none")
    }
}

impl ClipboardChain {
    /// Build the backend chain from configuration.
    pub fn detect(config: &ClipboardConfig) -> Self {
        let backends = match config.backend.as_str() {
            "arboard" => vec![Box::new(ArboardClipboard) as Box<dyn Clipboard>],
            "osc52" => vec![Box::new(Osc52Clipboard {
                max_bytes: config.max_osc52_bytes,
            }) as Box<dyn Clipboard>],
            "wl-copy" => vec![external_wl_copy()],
            "xclip" => vec![external_xclip()],
            "xsel" => vec![external_xsel()],
            // auto: prefer external tools that match the session, then native,
            // then OSC 52 as a last resort.
            _ => auto_backends(config),
        };
        Self { backends }
    }
}

fn auto_backends(config: &ClipboardConfig) -> Vec<Box<dyn Clipboard>> {
    let mut backends: Vec<Box<dyn Clipboard>> = Vec::new();
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let x11 = std::env::var_os("DISPLAY").is_some();

    if wayland && util::command_exists("wl-copy") {
        backends.push(external_wl_copy());
    }
    if x11 && util::command_exists("xclip") {
        backends.push(external_xclip());
    }
    if x11 && util::command_exists("xsel") {
        backends.push(external_xsel());
    }
    backends.push(Box::new(ArboardClipboard));
    backends.push(Box::new(Osc52Clipboard {
        max_bytes: config.max_osc52_bytes,
    }));
    backends
}

fn external_wl_copy() -> Box<dyn Clipboard> {
    Box::new(ExternalClipboard {
        name: "wl-copy",
        program: "wl-copy",
        args: &[],
    })
}

fn external_xclip() -> Box<dyn Clipboard> {
    Box::new(ExternalClipboard {
        name: "xclip",
        program: "xclip",
        args: &["-selection", "clipboard"],
    })
}

fn external_xsel() -> Box<dyn Clipboard> {
    Box::new(ExternalClipboard {
        name: "xsel",
        program: "xsel",
        args: &["--clipboard", "--input"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_rejects_oversized_text() {
        let backend = Osc52Clipboard { max_bytes: 4 };
        assert!(backend.copy("way too long").is_err());
    }
}
