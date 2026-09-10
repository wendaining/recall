pub mod queries;
pub mod schema;

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Thin wrapper around a SQLite connection to the recall database.
pub struct Db {
    pub conn: Connection,
}

impl Db {
    /// Open (creating if needed) the database at `path` and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating data dir {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening database {}", path.display()))?;
        schema::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// In-memory database, used by tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        schema::migrate(&conn)?;
        Ok(Self { conn })
    }
}

/// zstd level used for output compression.
const ZSTD_LEVEL: i32 = 3;

pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    zstd::stream::encode_all(data, ZSTD_LEVEL).context("zstd compress")
}

pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    zstd::stream::decode_all(data).context("zstd decompress")
}

/// Write `data` to `path` atomically-ish (used for debug dumps).
pub fn write_file(path: &Path, data: &[u8]) -> Result<()> {
    let mut f = std::fs::File::create(path)
        .with_context(|| format!("creating {}", path.display()))?;
    f.write_all(data)?;
    Ok(())
}
