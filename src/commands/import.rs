use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};

use crate::cli::{ImportArgs, ImportSource};
use crate::config::Config;
use crate::db::{Db, queries};
use crate::model::{Block, BlockKind};
use crate::util;

pub fn run(args: ImportArgs) -> Result<()> {
    match args.source {
        ImportSource::Atuin { path, days } => import_atuin(path, days),
    }
}

/// Backfill metadata (no output) from atuin's plaintext history database.
/// Existing `atuin_id`s are skipped so re-running is safe.
fn import_atuin(path: Option<PathBuf>, days: u32) -> Result<()> {
    let config = Config::load()?;
    let atuin_path = path.unwrap_or_else(default_atuin_db_path);
    if !atuin_path.exists() {
        bail!("atuin database not found at {}", atuin_path.display());
    }

    let db = Db::open(&config.general.db_path)?;
    let existing = existing_atuin_ids(&db)?;

    let atuin = Connection::open_with_flags(&atuin_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening atuin database {}", atuin_path.display()))?;

    let cutoff = if days == 0 {
        0
    } else {
        util::now_ns() - days as i64 * 86_400_000_000_000
    };

    let mut stmt = atuin.prepare(
        "SELECT id, timestamp, duration, exit, command, cwd, session, hostname, shell
           FROM history
          WHERE timestamp >= ?1
          ORDER BY timestamp ASC",
    )?;

    let rows = stmt.query_map([cutoff], |row| {
        Ok(AtuinRow {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            duration: row.get(2).unwrap_or(-1),
            exit: row.get(3).unwrap_or(-1),
            command: row.get(4)?,
            cwd: row.get(5)?,
            session: row.get(6)?,
            hostname: row.get(7)?,
            shell: row.get(8)?,
        })
    })?;

    let mut imported = 0usize;
    let mut skipped = 0usize;
    for row in rows {
        let row = row?;
        if existing.contains(&row.id) {
            skipped += 1;
            continue;
        }

        let block = Block {
            id: crate::commands::new_id(),
            atuin_id: Some(row.id),
            session: row.session,
            hostname: row.hostname.or_else(|| util::resolved_hostname(&config)),
            shell: row.shell,
            command: row.command,
            cwd: row.cwd,
            started_at: row.timestamp,
            duration_ns: (row.duration >= 0).then_some(row.duration),
            exit_code: (row.exit >= 0).then_some(row.exit as i32),
            output: None,
            output_bytes: 0,
            output_lines: 0,
            output_truncated: false,
            kind: BlockKind::Unavailable,
            created_at: util::now_ns(),
        };
        queries::insert(&db.conn, &block)?;
        imported += 1;
    }

    println!("imported {imported} block(s) from atuin ({skipped} already present)");
    Ok(())
}

fn default_atuin_db_path() -> PathBuf {
    atuin_data_dir(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        dirs::home_dir(),
    )
    .join("history.db")
}

fn atuin_data_dir(xdg_data_home: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    xdg_data_home
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| home.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("atuin")
}

struct AtuinRow {
    id: String,
    timestamp: i64,
    duration: i64,
    exit: i64,
    command: String,
    cwd: Option<String>,
    session: Option<String>,
    hostname: Option<String>,
    shell: Option<String>,
}

fn existing_atuin_ids(db: &Db) -> Result<HashSet<String>> {
    let mut stmt = db
        .conn
        .prepare("SELECT atuin_id FROM blocks WHERE atuin_id IS NOT NULL")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut ids = HashSet::new();
    for id in rows {
        ids.insert(id?);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn resolves_optional_importer_default_path() {
        let home = Path::new("home-base");

        assert_eq!(
            atuin_data_dir(None, Some(home.to_path_buf())),
            home.join(".local").join("share").join("atuin")
        );
        assert_eq!(
            atuin_data_dir(Some(PathBuf::from("relative")), Some(home.to_path_buf())),
            home.join(".local").join("share").join("atuin")
        );
        let xdg = if cfg!(windows) {
            PathBuf::from(r"C:\xdg")
        } else {
            PathBuf::from("/xdg")
        };
        assert_eq!(
            atuin_data_dir(Some(xdg.clone()), Some(home.to_path_buf())),
            xdg.join("atuin")
        );
    }
}
