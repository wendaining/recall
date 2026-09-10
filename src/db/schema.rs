use anyhow::Result;
use rusqlite::Connection;

/// Current schema version. Bump and add a migration branch when changing.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS blocks (
    id               TEXT PRIMARY KEY,
    atuin_id         TEXT,
    session          TEXT,
    hostname         TEXT,
    shell            TEXT,
    command          TEXT NOT NULL,
    cwd              TEXT,
    started_at       INTEGER NOT NULL,
    duration_ns      INTEGER,
    exit_code        INTEGER,
    output           BLOB,
    output_codec     TEXT,
    output_text      TEXT,
    output_bytes     INTEGER NOT NULL DEFAULT 0,
    output_lines     INTEGER NOT NULL DEFAULT 0,
    output_truncated INTEGER NOT NULL DEFAULT 0,
    kind             TEXT NOT NULL,
    created_at       INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_blocks_started ON blocks(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_blocks_atuin   ON blocks(atuin_id);
CREATE INDEX IF NOT EXISTS idx_blocks_cwd     ON blocks(cwd);
CREATE INDEX IF NOT EXISTS idx_blocks_session ON blocks(session, started_at);

CREATE VIRTUAL TABLE IF NOT EXISTS blocks_fts USING fts5(
    command,
    output_text,
    content='blocks',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS blocks_ai AFTER INSERT ON blocks BEGIN
    INSERT INTO blocks_fts(rowid, command, output_text)
    VALUES (new.rowid, new.command, new.output_text);
END;

CREATE TRIGGER IF NOT EXISTS blocks_ad AFTER DELETE ON blocks BEGIN
    INSERT INTO blocks_fts(blocks_fts, rowid, command, output_text)
    VALUES ('delete', old.rowid, old.command, old.output_text);
END;

CREATE TRIGGER IF NOT EXISTS blocks_au AFTER UPDATE ON blocks BEGIN
    INSERT INTO blocks_fts(blocks_fts, rowid, command, output_text)
    VALUES ('delete', old.rowid, old.command, old.output_text);
    INSERT INTO blocks_fts(rowid, command, output_text)
    VALUES (new.rowid, new.command, new.output_text);
END;
"#;

/// Configure connection pragmas and apply any pending migrations.
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;",
    )?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
