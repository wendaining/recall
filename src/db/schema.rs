use anyhow::Result;
use rusqlite::Connection;

/// Current schema version. Bump and add a migration branch when changing.
pub const SCHEMA_VERSION: i64 = 3;

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

const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const SCHEMA_V3: &str = r#"
CREATE TABLE IF NOT EXISTS block_imports (
    source      TEXT NOT NULL,
    external_id TEXT NOT NULL,
    block_id    TEXT NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    PRIMARY KEY (source, external_id)
);

CREATE INDEX IF NOT EXISTS idx_block_imports_block ON block_imports(block_id);

INSERT OR IGNORE INTO block_imports (source, external_id, block_id)
SELECT 'atuin', atuin_id, id FROM blocks WHERE atuin_id IS NOT NULL;
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
    if version < 2 {
        conn.execute_batch(SCHEMA_V2)?;
    }
    if version < 3 {
        conn.execute_batch(SCHEMA_V3)?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_database_to_current_schema() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO blocks (id, atuin_id, command, started_at, kind, created_at)
             VALUES ('block-1', 'legacy-1', 'echo hello', 1, 'empty', 1)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('ui.list_width_pct', '55')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO block_imports (source, external_id, block_id)
             VALUES ('history:zsh', 'entry-1', 'block-1')",
            [],
        )
        .unwrap();
        let migrated_block: String = conn
            .query_row(
                "SELECT block_id FROM block_imports
                  WHERE source = 'atuin' AND external_id = 'legacy-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migrated_block, "block-1");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }
}
