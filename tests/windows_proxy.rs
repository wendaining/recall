#![cfg(windows)]

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[test]
fn conpty_forwards_special_keys_and_exits_cleanly() {
    let test_dir = std::env::temp_dir().join(format!("recall-conpty-{}", std::process::id()));
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
    let mut writer = pair.master.take_writer().unwrap();
    drop(pair.master);

    let output = Arc::new(Mutex::new(Vec::new()));
    let reader_output = output.clone();
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(count) = reader.read(&mut chunk) {
            if count == 0 {
                break;
            }
            reader_output
                .lock()
                .unwrap()
                .extend_from_slice(&chunk[..count]);
        }
    });

    writer
        .write_all(b"Write-Output ('__RECALL_CONPTY_' + 'KEY__')\r")
        .unwrap();
    writer.flush().unwrap();
    require_occurrences(&output, "__RECALL_CONPTY_KEY__", 1, &mut child);

    // Up Arrow is delivered as VT input. PSReadLine should recall and execute
    // the previous command, proving non-text keys survive both ConPTY layers.
    writer.write_all(b"\x1b[A\r").unwrap();
    writer.flush().unwrap();
    require_occurrences(&output, "__RECALL_CONPTY_KEY__", 2, &mut child);

    writer.write_all(b"exit\r").unwrap();
    writer.flush().unwrap();

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

fn require_occurrences(
    output: &Arc<Mutex<Vec<u8>>>,
    needle: &str,
    expected: usize,
    child: &mut Box<dyn portable_pty::Child + Send + Sync>,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
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
