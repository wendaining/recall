use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

#[test]
fn imports_an_atuin_database_without_an_atuin_binary() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let test_dir =
        std::env::temp_dir().join(format!("recall-atuin-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&test_dir).unwrap();
    let source_path = test_dir.join("atuin.db");
    let config_path = test_dir.join("config.toml");
    let db_path = test_dir.join("recall.db");

    let source = Connection::open(&source_path).unwrap();
    source
        .execute_batch(
            "CREATE TABLE history (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                duration INTEGER,
                exit INTEGER,
                command TEXT NOT NULL,
                cwd TEXT,
                session TEXT,
                hostname TEXT,
                shell TEXT
            );
            INSERT INTO history
                (id, timestamp, duration, exit, command, cwd, session, hostname, shell)
            VALUES
                ('source-1', 1700000000000000000, 4000000, 0, 'echo imported',
                 '/tmp', 'session-1', 'host-1', 'zsh');",
        )
        .unwrap();
    drop(source);
    std::fs::write(
        &config_path,
        format!("[general]\ndb_path = '{}'\n", db_path.display()),
    )
    .unwrap();

    let run_import = || {
        Command::new(env!("CARGO_BIN_EXE_recall"))
            .args(["import", "atuin", "--path"])
            .arg(&source_path)
            .env("RECALL_CONFIG", &config_path)
            .env_remove("PATH")
            .output()
            .unwrap()
    };

    let first = run_import();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(String::from_utf8_lossy(&first.stdout).contains("imported 1 block(s)"));

    let second = run_import();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(String::from_utf8_lossy(&second.stdout).contains("imported 0 block(s)"));

    let conn = Connection::open(&db_path).unwrap();
    let imported: (String, String, i64, i64) = conn
        .query_row(
            "SELECT command, cwd, duration_ns, exit_code FROM blocks",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        imported,
        (
            "echo imported".to_string(),
            "/tmp".to_string(),
            4_000_000,
            0
        )
    );

    drop(conn);
    let _ = std::fs::remove_dir_all(test_dir);
}
