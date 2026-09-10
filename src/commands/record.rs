use anyhow::Result;

use crate::cli::RecordArgs;
use crate::config::Config;
use crate::db::{Db, queries};
use crate::model::{Block, BlockKind};
use crate::util;

/// Store metadata without output. Used by the shell integration when the proxy
/// is not active, so recall still has a usable history.
pub fn run(args: RecordArgs) -> Result<()> {
    let config = Config::load()?;
    let db = Db::open(&config.general.db_path)?;
    let now = util::now_ns();

    let block = Block {
        id: crate::commands::new_id(),
        atuin_id: args.atuin_id,
        session: args
            .session
            .or_else(|| std::env::var("RECALL_SESSION").ok()),
        hostname: util::resolved_hostname(&config),
        shell: Some("zsh".to_string()),
        command: args.command,
        cwd: args
            .cwd
            .or_else(|| std::env::current_dir().ok().map(|p| p.display().to_string())),
        started_at: now,
        duration_ns: args.duration_ns,
        exit_code: args.exit,
        output: None,
        output_codec: None,
        output_bytes: 0,
        output_lines: 0,
        output_truncated: false,
        kind: BlockKind::Unavailable,
        created_at: now,
    };

    queries::insert(&db.conn, &block)?;
    Ok(())
}
