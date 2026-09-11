use std::ffi::{OsStr, OsString};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::capture::classifier::{self, ClassifyInput};
use crate::capture::marker::{Feed, MarkerFilter, Op};
use crate::capture::protocol::Request;
use crate::capture::secrets;
use crate::config::Config;
use crate::db::{Db, queries};
use crate::model::{Block, BlockKind};
use crate::util;

/// In-flight capture for a single command.
struct Active {
    id: String,
    command: String,
    cwd: Option<String>,
    atuin_id: Option<String>,
    started_at: i64,
    buffer: Vec<u8>,
    truncated: bool,
    total: usize,
    max: usize,
    exclude_output: bool,
}

impl Active {
    fn push(&mut self, data: &[u8]) {
        self.total += data.len();
        if self.exclude_output {
            return;
        }
        if self.buffer.len() < self.max {
            let room = self.max - self.buffer.len();
            let take = data.len().min(room);
            self.buffer.extend_from_slice(&data[..take]);
            if take < data.len() {
                self.truncated = true;
            }
        } else {
            self.truncated = true;
        }
    }
}

#[derive(Default)]
struct CaptureState {
    active: Option<Active>,
}

/// State shared between the byte-forwarding loop and the writer thread.
struct Shared {
    state: Mutex<CaptureState>,
    config: Arc<Config>,
    session: String,
    shell: String,
    hostname: Option<String>,
    exclude: regex::RegexSet,
    exclude_output: regex::RegexSet,
    tx: Sender<Block>,
    pending: AtomicUsize,
    markers_seen: AtomicUsize,
    input_bytes: AtomicUsize,
}

/// Run the PTY proxy until the child shell exits. Returns the child exit code.
pub fn run(config: Arc<Config>, shell: String, login: bool) -> Result<i32> {
    let session = std::env::var("RECALL_SESSION")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(crate::commands::new_id);

    let shell_name = std::path::Path::new(&shell)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sh")
        .to_string();

    let (tx, rx) = mpsc::channel::<Block>();
    let shared = Arc::new(Shared {
        state: Mutex::new(CaptureState::default()),
        config: config.clone(),
        session: session.clone(),
        shell: shell_name,
        hostname: util::resolved_hostname(&config),
        exclude: build_exclude(&config.proxy.exclude),
        exclude_output: build_exclude(&config.proxy.exclude_output),
        tx,
        pending: AtomicUsize::new(0),
        markers_seen: AtomicUsize::new(0),
        input_bytes: AtomicUsize::new(0),
    });

    spawn_writer(rx, config.general.db_path.clone(), shared.clone());

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(terminal_size().unwrap_or(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }))?;

    let mut cmd = CommandBuilder::new(&shell);
    // `-l` is a Unix login-shell convention; PowerShell/cmd reject it.
    if login && cfg!(unix) {
        cmd.arg("-l");
    }
    cmd.env("RECALL_PROXY_ACTIVE", "1");
    cmd.env("RECALL_SESSION", &session);
    if let Some(path) = recall_path() {
        cmd.env("PATH", path);
    }
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let master = Arc::new(Mutex::new(pair.master));

    let _raw_guard = RawModeGuard::new();

    spawn_stdin_pump(writer, shared.clone());
    spawn_resize_handler(master);

    let debug = std::env::var_os("RECALL_DEBUG").is_some();
    let mut stdout = std::io::stdout();
    let mut buf = [0u8; 16384];
    let mut filter = MarkerFilter::new();
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                match filter.feed(chunk) {
                    Feed::Plain(bytes) => {
                        capture(&shared, bytes);
                        if stdout.write_all(bytes).is_err() {
                            break;
                        }
                    }
                    Feed::Ops(ops) => {
                        for op in ops {
                            match op {
                                Op::Bytes(bytes) => {
                                    capture(&shared, &bytes);
                                    if stdout.write_all(&bytes).is_err() {
                                        break;
                                    }
                                }
                                Op::Event(event) => {
                                    if debug {
                                        util::eprintln_flush("recall[debug]: marker event");
                                    }
                                    apply_event(&shared, event);
                                }
                            }
                        }
                    }
                }
                let _ = stdout.flush();
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    let rest = filter.flush();
    if !rest.is_empty() {
        capture(&shared, &rest);
        let _ = stdout.write_all(&rest);
        let _ = stdout.flush();
    }

    let code = child.wait().map(|status| status.exit_code()).unwrap_or(1) as i32;

    drain_pending(&shared, Duration::from_millis(500));

    drop(_raw_guard);

    if should_warn(
        shared.markers_seen.load(Ordering::SeqCst),
        shared.input_bytes.load(Ordering::SeqCst),
    ) {
        util::eprintln_flush("recall: no command markers were captured in this session.");
        util::eprintln_flush(&format!(
            "recall: add `eval \"$(recall init {})\"` to your shell startup file.",
            shared.shell
        ));
    }

    Ok(code)
}

fn should_warn(markers_seen: usize, input_bytes: usize) -> bool {
    markers_seen == 0 && input_bytes > 0
}

/// Apply a start/end marker to the capture state.
fn apply_event(shared: &Arc<Shared>, event: Request) {
    shared.markers_seen.fetch_add(1, Ordering::SeqCst);
    match event {
        Request::Start {
            id,
            command,
            cwd,
            atuin_id,
            started_at,
        } => {
            if !shared.exclude.is_match(&command) {
                let exclude_output = shared.exclude_output.is_match(&command);
                let mut state = shared.state.lock().unwrap();
                state.active = Some(Active {
                    id,
                    command,
                    cwd,
                    atuin_id,
                    started_at: started_at.unwrap_or_else(util::now_ns),
                    buffer: Vec::new(),
                    truncated: false,
                    total: 0,
                    max: shared.config.general.max_output_bytes,
                    exclude_output,
                });
            }
        }
        Request::End {
            id,
            exit,
            duration_ns,
        } => {
            let active = shared.state.lock().unwrap().active.take();
            if let Some(active) = active
                && active.id == id
            {
                let block = finalize(active, exit, duration_ns, shared);
                shared.pending.fetch_add(1, Ordering::SeqCst);
                let _ = shared.tx.send(block);
            }
        }
    }
}

/// Append forwarded bytes to the active capture.
fn capture(shared: &Arc<Shared>, data: &[u8]) {
    let mut state = shared.state.lock().unwrap();
    if let Some(active) = state.active.as_mut() {
        active.push(data);
    }
}

fn finalize(
    active: Active,
    exit: Option<i32>,
    duration_ns: Option<i64>,
    shared: &Arc<Shared>,
) -> Block {
    let mut classified = if active.exclude_output {
        classifier::Classified {
            kind: BlockKind::OutputExcluded,
            output: None,
            truncated: false,
        }
    } else {
        let interactive = classifier::detect_interactive(&active.buffer);
        classifier::classify(ClassifyInput {
            raw: &active.buffer,
            interactive,
            max_output_bytes: shared.config.general.max_output_bytes,
            strip_ansi: shared.config.general.strip_ansi,
            mark_interactive: shared.config.proxy.mark_interactive,
        })
    };

    if !active.exclude_output
        && shared.config.proxy.secrets_filter
        && looks_secret(&active.command, &classified.output)
    {
        classified = classifier::Classified {
            kind: BlockKind::Filtered,
            output: None,
            truncated: false,
        };
    }

    let output_lines = classified
        .output
        .as_ref()
        .map(|out| out.iter().filter(|&&b| b == b'\n').count() as i64)
        .unwrap_or(0);

    Block {
        id: active.id,
        atuin_id: active.atuin_id,
        session: Some(shared.session.clone()),
        hostname: shared.hostname.clone(),
        shell: Some(shared.shell.clone()),
        command: active.command,
        cwd: active.cwd,
        started_at: active.started_at,
        duration_ns,
        exit_code: exit,
        output: classified.output,
        output_bytes: active.total as i64,
        output_lines,
        output_truncated: classified.truncated || active.truncated,
        kind: classified.kind,
        created_at: util::now_ns(),
    }
}

fn recall_path() -> Option<OsString> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    merge_path(std::env::var_os("PATH").as_deref(), &dir)
}

fn merge_path(existing: Option<&OsStr>, dir: &Path) -> Option<OsString> {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(existing) = existing {
        paths.extend(std::env::split_paths(existing).filter(|path| path != dir));
    }
    std::env::join_paths(paths).ok()
}

fn build_exclude(patterns: &[String]) -> regex::RegexSet {
    let valid: Vec<&str> = patterns
        .iter()
        .map(String::as_str)
        .filter(|pattern| regex::Regex::new(pattern).is_ok())
        .collect();
    regex::RegexSet::new(valid).unwrap_or_else(|_| regex::RegexSet::new([r"$^"]).unwrap())
}

fn looks_secret(command: &str, output: &Option<Vec<u8>>) -> bool {
    if secrets::contains_secret(command) {
        return true;
    }
    match output {
        Some(bytes) => secrets::contains_secret(&String::from_utf8_lossy(bytes)),
        None => false,
    }
}

fn spawn_writer(rx: Receiver<Block>, db_path: PathBuf, shared: Arc<Shared>) {
    thread::spawn(move || {
        let db = match Db::open(&db_path) {
            Ok(db) => db,
            Err(err) => {
                util::eprintln_flush(&format!("recall: cannot open database: {err}"));
                return;
            }
        };
        while let Ok(block) = rx.recv() {
            if let Err(err) = queries::insert(&db.conn, &block) {
                util::eprintln_flush(&format!("recall: insert failed: {err}"));
            }
            shared.pending.fetch_sub(1, Ordering::SeqCst);
        }
    });
}

fn spawn_stdin_pump(mut writer: Box<dyn Write + Send>, shared: Arc<Shared>) {
    thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 8192];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    shared.input_bytes.fetch_add(n, Ordering::SeqCst);
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

#[cfg(unix)]
fn spawn_resize_handler(master: Arc<Mutex<Box<dyn MasterPty + Send>>>) {
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

#[cfg(not(unix))]
fn spawn_resize_handler(master: Arc<Mutex<Box<dyn MasterPty + Send>>>) {
    thread::spawn(move || {
        let mut last = crossterm::terminal::size().unwrap_or((0, 0));
        loop {
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

#[cfg(unix)]
fn terminal_size() -> Option<PtySize> {
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

#[cfg(unix)]
fn terminal_size_for_fd(fd: libc::c_int) -> Option<PtySize> {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::uninit();
    // SAFETY: `size` points to writable storage for `winsize`; the caller owns a
    // valid file descriptor for the duration of this call.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, size.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: a successful TIOCGWINSZ call initialized the full `winsize` value.
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

#[cfg(not(unix))]
fn terminal_size() -> Option<PtySize> {
    crossterm_terminal_size()
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

fn drain_pending(shared: &Arc<Shared>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while shared.pending.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
}

struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    fn new() -> Self {
        let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        let active = is_tty && crossterm::terminal::enable_raw_mode().is_ok();
        Self { active }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs::File;
    #[cfg(unix)]
    use std::os::fd::{AsRawFd, FromRawFd};

    use super::*;

    #[test]
    fn excluded_output_keeps_metadata_without_buffering() {
        let mut config = Config::default();
        config.proxy.exclude_output = vec![r"^tail -f".to_string()];
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(CaptureState::default()),
            config: Arc::new(config.clone()),
            session: "session-1".to_string(),
            shell: "zsh".to_string(),
            hostname: Some("host-1".to_string()),
            exclude: build_exclude(&config.proxy.exclude),
            exclude_output: build_exclude(&config.proxy.exclude_output),
            tx,
            pending: AtomicUsize::new(0),
            markers_seen: AtomicUsize::new(0),
            input_bytes: AtomicUsize::new(0),
        });

        apply_event(
            &shared,
            Request::Start {
                id: "block-1".to_string(),
                command: "tail -f app.log".to_string(),
                cwd: Some("/tmp".to_string()),
                atuin_id: None,
                started_at: Some(123),
            },
        );
        capture(&shared, b"a large stream of output\n");

        let state = shared.state.lock().unwrap();
        let active = state.active.as_ref().unwrap();
        assert!(active.buffer.is_empty());
        assert_eq!(active.total, 25);
        drop(state);

        apply_event(
            &shared,
            Request::End {
                id: "block-1".to_string(),
                exit: Some(0),
                duration_ns: Some(456),
            },
        );
        let block = rx.recv().unwrap();

        assert_eq!(block.command, "tail -f app.log");
        assert_eq!(block.cwd.as_deref(), Some("/tmp"));
        assert_eq!(block.started_at, 123);
        assert_eq!(block.duration_ns, Some(456));
        assert_eq!(block.exit_code, Some(0));
        assert_eq!(block.output_bytes, 25);
        assert!(block.output.is_none());
        assert!(!block.output_truncated);
        assert_eq!(block.kind, BlockKind::OutputExcluded);
    }

    #[test]
    fn merge_path_prepends_dir_and_drops_duplicate() {
        let dir = Path::new("/opt/recall/bin");
        let existing = std::env::join_paths(["/usr/bin", "/opt/recall/bin", "/bin"]).unwrap();

        let merged = merge_path(Some(existing.as_os_str()), dir).unwrap();
        let parts: Vec<PathBuf> = std::env::split_paths(&merged).collect();

        assert_eq!(
            parts,
            [
                PathBuf::from("/opt/recall/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        );
    }

    #[test]
    fn merge_path_handles_missing_existing_path() {
        let dir = Path::new("/opt/recall/bin");

        let merged = merge_path(None, dir).unwrap();
        let parts: Vec<PathBuf> = std::env::split_paths(&merged).collect();

        assert_eq!(parts, [PathBuf::from("/opt/recall/bin")]);
    }

    #[test]
    fn warns_only_when_input_arrives_without_markers() {
        assert!(should_warn(0, 8));
        assert!(!should_warn(1, 8));
        assert!(!should_warn(0, 0));
    }

    #[test]
    fn captures_a_start_end_stream_with_crlf_output() {
        let config = Config::default();
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(CaptureState::default()),
            config: Arc::new(config.clone()),
            session: "session-1".to_string(),
            shell: "pwsh".to_string(),
            hostname: None,
            exclude: build_exclude(&config.proxy.exclude),
            exclude_output: build_exclude(&config.proxy.exclude_output),
            tx,
            pending: AtomicUsize::new(0),
            markers_seen: AtomicUsize::new(0),
            input_bytes: AtomicUsize::new(0),
        });

        let mut stream = Vec::new();
        stream.extend_from_slice(
            b"\x1b]9999;{\"type\":\"start\",\"id\":\"b1\",\"command\":\"echo hi\",\"cwd\":\"C:/tmp\",\"started_at\":42}\x07",
        );
        stream.extend_from_slice(b"hi\r\nthere\r\n");
        stream.extend_from_slice(
            b"\x1b]9999;{\"type\":\"end\",\"id\":\"b1\",\"exit\":0,\"duration_ns\":7}\x07",
        );

        let mut filter = MarkerFilter::new();
        match filter.feed(&stream) {
            Feed::Plain(bytes) => capture(&shared, bytes),
            Feed::Ops(ops) => {
                for op in ops {
                    match op {
                        Op::Bytes(bytes) => capture(&shared, &bytes),
                        Op::Event(event) => apply_event(&shared, event),
                    }
                }
            }
        }

        let block = rx.recv().unwrap();
        assert_eq!(block.command, "echo hi");
        assert_eq!(block.cwd.as_deref(), Some("C:/tmp"));
        assert_eq!(block.started_at, 42);
        assert_eq!(block.exit_code, Some(0));
        assert_eq!(block.duration_ns, Some(7));
        assert_eq!(block.shell.as_deref(), Some("pwsh"));
        // The ANSI stripper normalizes the CRLF line endings ConPTY emits.
        assert_eq!(block.output.as_deref(), Some(b"hi\nthere".as_slice()));
    }

    #[cfg(unix)]
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
        // SAFETY: all output pointers are valid, and `expected` is fully initialized.
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
