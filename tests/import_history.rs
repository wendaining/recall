use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

#[test]
fn imports_a_shell_history_file_idempotently() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let test_dir =
        std::env::temp_dir().join(format!("recall-import-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&test_dir).unwrap();
    let history_path = test_dir.join("bash_history");
    let config_path = test_dir.join("config.toml");
    let db_path = test_dir.join("recall.db");
    std::fs::write(
        &history_path,
        "#1700000000\necho first\n#1700000001\nprintf second\n",
    )
    .unwrap();
    std::fs::write(
        &config_path,
        format!("[general]\ndb_path = '{}'\n", db_path.display()),
    )
    .unwrap();

    let run_import = || {
        Command::new(env!("CARGO_BIN_EXE_recall"))
            .args(["import", "history", "bash", "--path"])
            .arg(&history_path)
            .env("RECALL_CONFIG", &config_path)
            .output()
            .unwrap()
    };

    let first = run_import();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(String::from_utf8_lossy(&first.stdout).contains("imported 2 block(s)"));

    let second = run_import();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(String::from_utf8_lossy(&second.stdout).contains("imported 0 block(s)"));
    assert!(String::from_utf8_lossy(&second.stdout).contains("2 already present"));

    let conn = Connection::open(&db_path).unwrap();
    let commands = conn
        .prepare("SELECT command FROM blocks ORDER BY started_at")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(commands, ["echo first", "printf second"]);

    drop(conn);
    let _ = std::fs::remove_dir_all(test_dir);
}
