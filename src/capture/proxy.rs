use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::capture::classifier::{self, ClassifyInput};
use crate::capture::marker::{Feed, MarkerFilter, Op};
use crate::capture::protocol::Request;
use crate::capture::secrets;
use crate::config::Config;
use crate::db::{Db, queries};
use crate::model::{Block, BlockKind};
use crate::platform;
use crate::util;

/// In-flight capture for a single command.
struct Active {
    id: String,
    command: String,
    cwd: Option<String>,
    started_at: i64,
    buffer: Vec<u8>,
    truncated: bool,
    total: usize,
    max: usize,
    exclude_output: bool,
    /// Set once the prompt begins; prompt bytes are not part of the output.
    discarding: bool,
}

impl Active {
    fn push(&mut self, data: &[u8]) {
        if self.discarding {
            return;
        }
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

enum WriterMessage {
    Block(Box<Block>),
    Shutdown,
}

const WRITE_RETRY_DELAYS: [Duration; 4] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(250),
    Duration::from_millis(500),
];

/// State shared between the byte-forwarding loop and the writer thread.
struct Shared {
    state: Mutex<CaptureState>,
    config: Arc<Config>,
    session: String,
    shell: String,
    hostname: Option<String>,
    exclude: regex::RegexSet,
    exclude_output: regex::RegexSet,
    tx: Sender<WriterMessage>,
    markers_seen: AtomicUsize,
    input_bytes: Arc<AtomicUsize>,
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

    let (tx, rx) = mpsc::channel::<WriterMessage>();
    let shared = Arc::new(Shared {
        state: Mutex::new(CaptureState::default()),
        config: config.clone(),
        session: session.clone(),
        shell: shell_name,
        hostname: util::resolved_hostname(&config),
        exclude: build_exclude(&config.proxy.exclude),
        exclude_output: build_exclude(&config.proxy.exclude_output),
        tx,
        markers_seen: AtomicUsize::new(0),
        input_bytes: Arc::new(AtomicUsize::new(0)),
    });

    let writer_thread = spawn_writer(rx, config.general.db_path.clone());

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(platform::terminal_size().unwrap_or(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }))?;

    let mut cmd = CommandBuilder::new(&shell);
    platform::configure_shell(&mut cmd, login);
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
    let running = Arc::new(AtomicBool::new(true));

    let raw_guard = platform::RawModeGuard::new();

    platform::spawn_input_forwarder(writer, shared.input_bytes.clone());
    platform::spawn_resize_handler(master.clone(), running.clone());
    // The reader holds an OS pipe handle, not the pseudoconsole. Dropping the
    // last `master` closes ConPTY, which is what finally delivers EOF to the
    // reader on Windows; the resize thread owns the remaining reference.
    drop(master);

    let (code_tx, code_rx) = mpsc::channel::<i32>();
    {
        let running = running.clone();
        thread::spawn(move || {
            let code = child.wait().map(|status| status.exit_code()).unwrap_or(1) as i32;
            running.store(false, Ordering::SeqCst);
            let _ = code_tx.send(code);
        });
    }

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

    // Stop the resize thread so it releases the pseudoconsole, then collect the
    // exit code the waiter thread captured.
    running.store(false, Ordering::SeqCst);
    let code = code_rx.recv().unwrap_or(1);

    let _ = shared.tx.send(WriterMessage::Shutdown);
    let _ = writer_thread.join();

    drop(raw_guard);

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
            started_at,
        } => {
            if !shared.exclude.is_match(&command) {
                let exclude_output = shared.exclude_output.is_match(&command);
                let mut state = shared.state.lock().unwrap();
                state.active = Some(Active {
                    id,
                    command,
                    cwd,
                    started_at: started_at.unwrap_or_else(util::now_ns),
                    buffer: Vec::new(),
                    truncated: false,
                    total: 0,
                    max: shared.config.general.max_output_bytes,
                    exclude_output,
                    discarding: false,
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
                let _ = shared.tx.send(WriterMessage::Block(Box::new(block)));
            }
        }
        Request::Prompt => {
            if let Some(active) = shared.state.lock().unwrap().active.as_mut() {
                active.discarding = true;
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

fn spawn_writer(rx: Receiver<WriterMessage>, db_path: PathBuf) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let db = match retry_on_busy(|| Db::open(&db_path), thread::sleep) {
            Ok(db) => db,
            Err(err) => {
                util::eprintln_flush(&format!("recall: cannot open database: {err}"));
                return;
            }
        };
        while let Ok(message) = rx.recv() {
            match message {
                WriterMessage::Block(block) => {
                    if let Err(err) =
                        retry_on_busy(|| queries::insert(&db.conn, &block), thread::sleep)
                    {
                        util::eprintln_flush(&format!("recall: insert failed: {err}"));
                    }
                }
                WriterMessage::Shutdown => break,
            }
        }
    })
}

fn retry_on_busy<T>(
    mut operation: impl FnMut() -> Result<T>,
    mut wait: impl FnMut(Duration),
) -> Result<T> {
    for delay in WRITE_RETRY_DELAYS {
        match operation() {
            Err(err) if is_busy_or_locked(&err) => wait(delay),
            result => return result,
        }
    }
    operation()
}

fn is_busy_or_locked(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<rusqlite::Error>())
        .any(|cause| {
            matches!(
                cause.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("recall-proxy-{}-{nonce}", std::process::id()))
            .join("recall.db")
    }

    fn sqlite_busy_error() -> anyhow::Error {
        rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY), None)
            .into()
    }

    #[test]
    fn retries_busy_operations_with_backoff() {
        let mut attempts = 0;
        let mut waits = Vec::new();

        retry_on_busy(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(sqlite_busy_error())
                } else {
                    Ok(())
                }
            },
            |delay| waits.push(delay),
        )
        .unwrap();

        assert_eq!(attempts, 3);
        assert_eq!(waits, WRITE_RETRY_DELAYS[..2]);
    }

    #[test]
    fn does_not_retry_permanent_failures() {
        let mut attempts = 0;

        let err = retry_on_busy(
            || {
                attempts += 1;
                Err::<(), _>(anyhow::anyhow!("permanent failure"))
            },
            |_| panic!("permanent failures must not wait"),
        )
        .unwrap_err();

        assert_eq!(attempts, 1);
        assert_eq!(err.to_string(), "permanent failure");
    }

    #[test]
    fn writer_shutdown_flushes_queued_blocks() {
        let db_path = temp_db_path();
        let (tx, rx) = mpsc::channel();
        let writer = spawn_writer(rx, db_path.clone());
        let block = Block {
            id: "block-1".to_string(),
            command: "echo queued".to_string(),
            started_at: 1,
            kind: BlockKind::Empty,
            created_at: 1,
            ..Default::default()
        };

        tx.send(WriterMessage::Block(Box::new(block))).unwrap();
        tx.send(WriterMessage::Shutdown).unwrap();
        writer.join().unwrap();

        let db = Db::open(&db_path).unwrap();
        assert_eq!(queries::count(&db.conn).unwrap(), 1);
        drop(db);
        std::fs::remove_dir_all(db_path.parent().unwrap()).unwrap();
    }

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
            markers_seen: AtomicUsize::new(0),
            input_bytes: Arc::new(AtomicUsize::new(0)),
        });

        apply_event(
            &shared,
            Request::Start {
                id: "block-1".to_string(),
                command: "tail -f app.log".to_string(),
                cwd: Some("/tmp".to_string()),
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
        let WriterMessage::Block(block) = rx.recv().unwrap() else {
            panic!("expected captured block");
        };

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
            markers_seen: AtomicUsize::new(0),
            input_bytes: Arc::new(AtomicUsize::new(0)),
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

        let WriterMessage::Block(block) = rx.recv().unwrap() else {
            panic!("expected captured block");
        };
        assert_eq!(block.command, "echo hi");
        assert_eq!(block.cwd.as_deref(), Some("C:/tmp"));
        assert_eq!(block.started_at, 42);
        assert_eq!(block.exit_code, Some(0));
        assert_eq!(block.duration_ns, Some(7));
        assert_eq!(block.shell.as_deref(), Some("pwsh"));
        // The ANSI stripper normalizes the CRLF line endings ConPTY emits.
        assert_eq!(block.output.as_deref(), Some(b"hi\nthere".as_slice()));
    }
}
