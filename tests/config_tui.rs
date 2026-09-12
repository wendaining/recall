#![cfg(unix)]

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[test]
fn config_without_a_terminal_explains_the_alternatives() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_recall"))
        .arg("config")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("interactive configuration requires a terminal"));
    assert!(error.contains("recall config show"));
}

#[test]
fn config_tui_records_a_key_and_saves_a_toggle() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let test_dir =
        std::env::temp_dir().join(format!("recall-config-tui-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&test_dir).unwrap();
    let config_path = test_dir.join("config.toml");
    std::fs::write(&config_path, "# keep\n[ui]\nsearch_key = \"alt-r\"\n").unwrap();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_recall"));
    command.arg("config");
    command.env("HOME", &test_dir);
    command.env("SHELL", "bash");
    command.env("TERM", "xterm-256color");
    command.env("RECALL_CONFIG", &config_path);

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

    require_output(&output, "Categories", &mut child);
    write_input(&writer, b"\t\r\x1bt\r\t q");

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().unwrap() {
            Some(status) => break status,
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            None => {
                let snapshot = output_text(&output);
                let _ = child.kill();
                panic!("config TUI did not exit:\n{snapshot}");
            }
        }
    };
    assert_eq!(status.exit_code(), 0, "{}", output_text(&output));
    drop(writer);
    drop(master);
    reader_thread.join().unwrap();

    let saved = std::fs::read_to_string(&config_path).unwrap();
    let config: toml::Value = toml::from_str(&saved).unwrap();
    assert_eq!(config["ui"]["search_key"].as_str(), Some("alt-t"));
    assert_eq!(config["proxy"]["secrets_filter"].as_bool(), Some(false));
    assert!(saved.contains("# keep"));
    let _ = std::fs::remove_dir_all(test_dir);
}

fn write_input(writer: &Arc<Mutex<Box<dyn Write + Send>>>, bytes: &[u8]) {
    let mut writer = writer.lock().unwrap();
    writer.write_all(bytes).unwrap();
    writer.flush().unwrap();
}

fn require_output(
    output: &Arc<Mutex<Vec<u8>>>,
    needle: &str,
    child: &mut Box<dyn portable_pty::Child + Send + Sync>,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if output_text(output).contains(needle) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let snapshot = output_text(output);
    let _ = child.kill();
    panic!("expected {needle} in config TUI output:\n{snapshot}");
}

fn output_text(output: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&output.lock().unwrap()).into_owned()
}
