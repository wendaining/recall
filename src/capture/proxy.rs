use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
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
}

impl Active {
    fn push(&mut self, data: &[u8]) {
        self.total += data.len();
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
    tx: Sender<Block>,
    pending: AtomicUsize,
}

/// Run the PTY proxy until the child shell exits. Returns the child exit code.
pub fn run(config: Arc<Config>, shell: String) -> Result<i32> {
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
        tx,
        pending: AtomicUsize::new(0),
    });

    spawn_writer(rx, config.general.db_path.clone(), shared.clone());

    let pty_system = native_pty_system();
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let pair = pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("RECALL_PROXY_ACTIVE", "1");
    cmd.env("RECALL_SESSION", &session);
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let master = Arc::new(Mutex::new(pair.master));

    let _raw_guard = RawModeGuard::new();

    spawn_stdin_pump(writer);
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
    Ok(code)
}

/// Apply a start/end marker to the capture state.
fn apply_event(shared: &Arc<Shared>, event: Request) {
    match event {
        Request::Start {
            id,
            command,
            cwd,
            atuin_id,
            started_at,
        } => {
            if !shared.exclude.is_match(&command) {
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
    let interactive = classifier::detect_interactive(&active.buffer);
    let mut classified = classifier::classify(ClassifyInput {
        raw: &active.buffer,
        interactive,
        max_output_bytes: shared.config.general.max_output_bytes,
        strip_ansi: shared.config.general.strip_ansi,
        mark_interactive: shared.config.proxy.mark_interactive,
    });

    if shared.config.proxy.secrets_filter && looks_secret(&active.command, &classified.output) {
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

fn spawn_stdin_pump(mut writer: Box<dyn Write + Send>) {
    thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 8192];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
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
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        let master = master.lock().unwrap();
        let _ = master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
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
