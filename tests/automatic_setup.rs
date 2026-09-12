#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use rusqlite::Connection;

#[test]
fn automatic_setup_loads_the_rc_body_once_and_captures_output() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let test_dir =
        std::env::temp_dir().join(format!("recall-auto-setup-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&test_dir).unwrap();
    let profile_path = test_dir.join(".bashrc");
    let config_path = test_dir.join("config.toml");
    let db_path = test_dir.join("recall.db");
    std::fs::write(
        &profile_path,
        "printf '__RECALL_RC_BODY__\\n'\nPS1='__RECALL_PROMPT__ '\n",
    )
    .unwrap();
    std::fs::write(
        &config_path,
        format!(
            "[general]\ndb_path = '{}'\n\n[proxy]\nlogin_shell = false\n",
            db_path.display()
        ),
    )
    .unwrap();

    let setup = std::process::Command::new(env!("CARGO_BIN_EXE_recall"))
        .args(["setup", "bash", "--profile"])
        .arg(&profile_path)
        .output()
        .unwrap();
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new("bash");
    command.args(["--noprofile", "--rcfile"]);
    command.arg(profile_path.as_os_str());
    command.arg("-i");
    command.env("HOME", &test_dir);
    command.env("SHELL", "bash");
    command.env("TERM", "xterm-256color");
    command.env("RECALL_CONFIG", &config_path);
    command.env("RECALL_PROXY_ACTIVE", "");
    command.env("RECALL_AUTO_LAUNCH", "");
    command.env("RECALL_PROXY", "1");
    command.env("RECALL_SESSION", "");

    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
    let master = pair.master;
    let output = Arc::new(Mutex::new(Vec::new()));
    let reader_output = output.clone();
    let reader_thread = thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => reader_output
                    .lock()
                    .unwrap()
                    .extend_from_slice(&chunk[..count]),
                Err(_) => break,
            }
        }
    });

    require_occurrences(&output, "__RECALL_PROMPT__", 1, &mut child);
    write_input(&writer, b"printf '__RECALL_CAPTURED_OUTPUT__\\n'\n");
    require_occurrences(&output, "__RECALL_CAPTURED_OUTPUT__", 2, &mut child);
    require_occurrences(&output, "__RECALL_PROMPT__", 2, &mut child);
    write_input(&writer, b"exit\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        match child.try_wait().unwrap() {
            Some(status) => break status,
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            None => {
                let snapshot = output_text(&output);
                let _ = child.kill();
                panic!("automatic setup did not exit after bash exited:\n{snapshot}");
            }
        }
    };
    assert_eq!(status.exit_code(), 0, "{}", output_text(&output));
    drop(writer);
    drop(master);
    reader_thread.join().unwrap();

    let snapshot = output_text(&output);
    assert_eq!(
        snapshot.matches("__RECALL_RC_BODY__").count(),
        1,
        "{snapshot}"
    );
    let conn = Connection::open(&db_path).unwrap();
    let (command, output): (String, Vec<u8>) = conn
        .query_row(
            "SELECT command, output FROM blocks WHERE command LIKE '%RECALL_CAPTURED_OUTPUT%'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(command.contains("__RECALL_CAPTURED_OUTPUT__"));
    let decoded = zstd::stream::decode_all(output.as_slice()).unwrap();
    assert!(String::from_utf8_lossy(&decoded).contains("__RECALL_CAPTURED_OUTPUT__"));

    drop(conn);
    let _ = std::fs::remove_dir_all(test_dir);
}

fn write_input(writer: &Arc<Mutex<Box<dyn Write + Send>>>, bytes: &[u8]) {
    let mut writer = writer.lock().unwrap();
    writer.write_all(bytes).unwrap();
    writer.flush().unwrap();
}

fn require_occurrences(
    output: &Arc<Mutex<Vec<u8>>>,
    needle: &str,
    expected: usize,
    child: &mut Box<dyn portable_pty::Child + Send + Sync>,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if output_text(output).matches(needle).count() >= expected {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let snapshot = output_text(output);
    let _ = child.kill();
    panic!("expected {expected} occurrence(s) of {needle}:\n{snapshot}");
}

fn output_text(output: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&output.lock().unwrap()).into_owned()
}
