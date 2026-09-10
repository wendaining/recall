use anyhow::{Context, Result};
use rusqlite::{Connection, Row, params};
use std::str::FromStr;

use crate::db::{compress, decompress};
use crate::model::{Block, BlockKind};

/// Maximum number of plain-text output bytes kept in the searchable projection.
const SEARCH_PROJECTION_BYTES: usize = 64 * 1024;

const COLUMNS: &str = "id, atuin_id, session, hostname, shell, command, cwd, \
     started_at, duration_ns, exit_code, output, output_codec, output_text, \
     output_bytes, output_lines, output_truncated, kind, created_at";

fn row_to_block(row: &Row<'_>, full_output: bool) -> rusqlite::Result<Block> {
    let kind_str: String = row.get("kind")?;
    let output_blob: Option<Vec<u8>> = row.get("output")?;
    let output_text: Option<String> = row.get("output_text")?;

    let output = if full_output {
        match output_blob {
            Some(blob) => match decompress(&blob) {
                Ok(raw) => Some(raw),
                Err(_) => output_text.clone().map(String::into_bytes),
            },
            None => None,
        }
    } else {
        output_text.map(String::into_bytes)
    };

    Ok(Block {
        id: row.get("id")?,
        atuin_id: row.get("atuin_id")?,
        session: row.get("session")?,
        hostname: row.get("hostname")?,
        shell: row.get("shell")?,
        command: row.get("command")?,
        cwd: row.get("cwd")?,
        started_at: row.get("started_at")?,
        duration_ns: row.get("duration_ns")?,
        exit_code: row.get("exit_code")?,
        output,
        output_codec: row.get("output_codec")?,
        output_bytes: row.get("output_bytes")?,
        output_lines: row.get("output_lines")?,
        output_truncated: row.get::<_, i64>("output_truncated")? != 0,
        kind: BlockKind::from_str(&kind_str).unwrap_or(BlockKind::Normal),
        created_at: row.get("created_at")?,
    })
}

/// Insert a block, compressing its output and populating the search projection.
pub fn insert(conn: &Connection, block: &Block) -> Result<()> {
    let (blob, codec, text) = match &block.output {
        Some(raw) if !raw.is_empty() => {
            let compressed = compress(raw)?;
            let projection_len = raw.len().min(SEARCH_PROJECTION_BYTES);
            let projection =
                String::from_utf8_lossy(&raw[..projection_len]).into_owned();
            (Some(compressed), Some("zstd".to_string()), Some(projection))
        }
        _ => (None, None, None),
    };

    conn.execute(
        "INSERT INTO blocks (id, atuin_id, session, hostname, shell, command, cwd,
            started_at, duration_ns, exit_code, output, output_codec, output_text,
            output_bytes, output_lines, output_truncated, kind, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            block.id,
            block.atuin_id,
            block.session,
            block.hostname,
            block.shell,
            block.command,
            block.cwd,
            block.started_at,
            block.duration_ns,
            block.exit_code,
            blob,
            codec,
            text,
            block.output_bytes,
            block.output_lines,
            block.output_truncated as i64,
            block.kind.as_str(),
            block.created_at,
        ],
    )
    .context("inserting block")?;
    Ok(())
}

/// Most recent blocks (without full output; preview comes from the projection).
pub fn recent(conn: &Connection, limit: usize) -> Result<Vec<Block>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM blocks ORDER BY started_at DESC LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit as i64], |row| row_to_block(row, false))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Fetch a single block with its full decompressed output.
pub fn get(conn: &Connection, id: &str) -> Result<Option<Block>> {
    let sql = format!("SELECT {COLUMNS} FROM blocks WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], |row| row_to_block(row, true))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// Search command text and output. Falls back to `LIKE` when the FTS query is
/// not usable (short terms, CJK < 3 chars, or syntax issues).
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Block>> {
    let query = query.trim();
    if query.is_empty() {
        return recent(conn, limit);
    }

    if let Some(match_expr) = fts_match_expr(query) {
        match fts_search(conn, &match_expr, limit) {
            Ok(blocks) if !blocks.is_empty() => return Ok(blocks),
            Ok(_) => {}
            Err(_) => {}
        }
    }
    like_search(conn, query, limit)
}

fn fts_match_expr(query: &str) -> Option<String> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return None;
    }
    // The trigram tokenizer cannot match terms shorter than three characters.
    if terms.iter().any(|t| t.chars().count() < 3) {
        return None;
    }
    Some(
        terms
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

fn fts_search(conn: &Connection, match_expr: &str, limit: usize) -> Result<Vec<Block>> {
    let sql = format!(
        "SELECT {} FROM blocks b
         JOIN blocks_fts f ON f.rowid = b.rowid
         WHERE f MATCH ?1
         ORDER BY b.started_at DESC LIMIT ?2",
        COLUMNS
            .split(", ")
            .map(|c| format!("b.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![match_expr, limit as i64], |row| {
        row_to_block(row, false)
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn like_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Block>> {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let sql = format!(
        "SELECT {COLUMNS} FROM blocks
         WHERE command LIKE ?1 ESCAPE '\\' OR output_text LIKE ?1 ESCAPE '\\'
         ORDER BY started_at DESC LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![pattern, limit as i64], |row| {
        row_to_block(row, false)
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM blocks", [], |row| row.get(0))?)
}

/// Drop stored output (full and projection) older than `days`. Returns the
/// number of blocks affected. `days == 0` disables expiry.
pub fn prune(conn: &Connection, days: u32, now_ns: i64) -> Result<usize> {
    if days == 0 {
        return Ok(0);
    }
    let cutoff = now_ns - (days as i64) * 86_400 * 1_000_000_000;
    let affected = conn.execute(
        "UPDATE blocks
            SET output = NULL, output_codec = NULL, output_text = NULL
          WHERE output IS NOT NULL AND started_at < ?1",
        params![cutoff],
    )?;
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::model::{Block, BlockKind};

    fn sample(id: &str, command: &str, output: Option<&str>, started_at: i64) -> Block {
        Block {
            id: id.to_string(),
            command: command.to_string(),
            cwd: Some("/tmp".to_string()),
            started_at,
            exit_code: Some(0),
            output: output.map(|s| s.as_bytes().to_vec()),
            output_bytes: output.map(|s| s.len() as i64).unwrap_or(0),
            output_lines: output.map(|s| s.lines().count() as i64).unwrap_or(0),
            kind: if output.is_some() {
                BlockKind::Normal
            } else {
                BlockKind::Empty
            },
            created_at: started_at,
            ..Default::default()
        }
    }

    #[test]
    fn insert_get_roundtrip_preserves_output() {
        let db = Db::open_in_memory().unwrap();
        let block = sample("a", "echo hello", Some("hello\nworld\n"), 1_000);
        insert(&db.conn, &block).unwrap();

        assert_eq!(count(&db.conn).unwrap(), 1);
        let got = get(&db.conn, "a").unwrap().unwrap();
        assert_eq!(got.command, "echo hello");
        assert_eq!(got.output.as_deref(), Some(b"hello\nworld\n".as_slice()));
        assert_eq!(got.exit_code, Some(0));
        assert!(got.has_output());
    }

    #[test]
    fn search_matches_command_and_output() {
        let db = Db::open_in_memory().unwrap();
        insert(&db.conn, &sample("a", "cargo build", Some("Compiling recall"), 1)).unwrap();
        insert(&db.conn, &sample("b", "git status", Some("nothing to commit"), 2)).unwrap();

        let by_cmd = search(&db.conn, "cargo", 10).unwrap();
        assert_eq!(by_cmd.len(), 1);
        assert_eq!(by_cmd[0].id, "a");

        let by_output = search(&db.conn, "nothing", 10).unwrap();
        assert_eq!(by_output.len(), 1);
        assert_eq!(by_output[0].id, "b");
    }

    #[test]
    fn search_supports_short_terms_via_like() {
        let db = Db::open_in_memory().unwrap();
        insert(&db.conn, &sample("a", "ls -la", Some("total 0"), 1)).unwrap();
        let hits = search(&db.conn, "ls", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn prune_drops_output_but_keeps_metadata() {
        let db = Db::open_in_memory().unwrap();
        insert(&db.conn, &sample("old", "echo old", Some("old output"), 1)).unwrap();
        let now = 100 * 86_400 * 1_000_000_000i64;
        let affected = prune(&db.conn, 30, now).unwrap();
        assert_eq!(affected, 1);

        let got = get(&db.conn, "old").unwrap().unwrap();
        assert!(got.output.is_none());
        assert_eq!(got.command, "echo old");
    }
}
