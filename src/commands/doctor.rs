use anyhow::Result;
use rusqlite::{Connection, OpenFlags};

use crate::config::Config;
use crate::db::{Db, queries, schema};
use crate::util;

pub fn run() -> Result<()> {
    let cfg = Config::load()?;

    println!("recall {}", env!("CARGO_PKG_VERSION"));
    println!("config:  {}", Config::config_path().display());
    println!("shell:   {}", util::login_shell());
    println!(
        "hostname: {}",
        util::resolved_hostname(&cfg).unwrap_or_else(|| "?".into())
    );
    println!("TERM:    {}", std::env::var("TERM").unwrap_or_default());
    println!();

    check_recall_db(&cfg)?;
    check_atuin_db(&cfg);
    check_clipboard(&cfg);

    Ok(())
}

fn check_recall_db(cfg: &Config) -> Result<()> {
    let path = &cfg.general.db_path;
    println!("[recall db] {}", path.display());
    match Db::open(path) {
        Ok(db) => {
            let count = queries::count(&db.conn).unwrap_or(-1);
            let version: i64 = db
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap_or(-1);
            println!("  ok: {count} block(s), schema v{version} (expected v{})", schema::SCHEMA_VERSION);
        }
        Err(err) => println!("  ERROR: {err}"),
    }
    Ok(())
}

fn check_atuin_db(cfg: &Config) {
    let path = &cfg.general.atuin_db_path;
    println!("[atuin db]  {}", path.display());
    if !path.exists() {
        println!("  not found (import/linking disabled)");
        return;
    }
    match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => match conn.query_row("SELECT COUNT(*) FROM history", [], |r| {
            r.get::<_, i64>(0)
        }) {
            Ok(count) => println!("  ok: {count} history row(s)"),
            Err(err) => println!("  cannot read history table: {err}"),
        },
        Err(err) => println!("  cannot open: {err}"),
    }
}

fn check_clipboard(cfg: &Config) {
    println!("[clipboard] configured backend: {}", cfg.clipboard.backend);
    for tool in ["wl-copy", "xclip", "xsel"] {
        println!(
            "  {tool:<8} {}",
            if util::command_exists(tool) { "found" } else { "missing" }
        );
    }
    println!("  arboard   built-in (native)");
    println!("  osc52     always available inside a supporting terminal");
    println!(
        "  session:  {} / {}",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".into()),
        std::env::var("WAYLAND_DISPLAY")
            .or_else(|_| std::env::var("DISPLAY"))
            .unwrap_or_else(|_| "no display".into())
    );
}
