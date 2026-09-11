#![cfg(windows)]

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[test]
fn conpty_forwards_special_keys_and_exits_cleanly() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let test_dir =
        std::env::temp_dir().join(format!("recall-conpty-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&test_dir).unwrap();
    let config_path = test_dir.join("config.toml");
    let db_path = test_dir.join("recall.db");
    std::fs::write(
        &config_path,
        format!("[general]\ndb_path = '{}'\n", db_path.display()),
    )
    .unwrap();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_recall"));
    command.args(["proxy", "--shell", "pwsh"]);
    command.env("RECALL_CONFIG", &config_path);
    command.env("POWERSHELL_TELEMETRY_OPTOUT", "1");

    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
    drop(pair.master);

    let output = Arc::new(Mutex::new(Vec::new()));
    let reader_output = output.clone();
    let terminal_writer = writer.clone();
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        let mut cursor_queries_answered = 0;
        while let Ok(count) = reader.read(&mut chunk) {
            if count == 0 {
                break;
            }
            let cursor_queries = {
                let mut output = reader_output.lock().unwrap();
                output.extend_from_slice(&chunk[..count]);
                output
                    .windows(b"\x1b[6n".len())
                    .filter(|window| *window == b"\x1b[6n")
                    .count()
            };

            while cursor_queries_answered < cursor_queries {
                let mut writer = terminal_writer.lock().unwrap();
                if writer.write_all(b"\x1b[1;1R").is_err() || writer.flush().is_err() {
                    return;
                }
                cursor_queries_answered += 1;
            }
        }
    });

    require_occurrences(&output, "PS ", 1, &mut child);

    write_input(
        &writer,
        b"function global:prompt { '__RECALL_' + 'READY__ ' }; Write-Output ('__RECALL_CONPTY_' + 'KEY__')\r",
    );
    require_occurrences(&output, "__RECALL_CONPTY_KEY__", 1, &mut child);
    require_occurrences(&output, "__RECALL_READY__", 1, &mut child);

    // PowerShell enables win32-input-mode through the proxied output, so a
    // terminal encodes Up Arrow as key-down/up INPUT_RECORD sequences.
    write_input(&writer, b"\x1b[38;72;0;1;256;1_\x1b[38;72;0;0;256;1_\r");
    require_occurrences(&output, "__RECALL_CONPTY_KEY__", 2, &mut child);
    require_occurrences(&output, "__RECALL_READY__", 2, &mut child);

    write_input(&writer, b"exit\r");

    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        match child.try_wait().unwrap() {
            Some(status) => break status,
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            None => {
                let snapshot = output_text(&output);
                let _ = child.kill();
                panic!("proxy did not exit after PowerShell exited:\n{snapshot}");
            }
        }
    };

    assert_eq!(status.exit_code(), 0, "{}", output_text(&output));
    drop(writer);
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
