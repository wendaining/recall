mod doctor;
mod import;
mod init;
mod proxy;
mod record;
mod setup;
mod shell;
mod update;

use anyhow::Result;

use crate::cli::{Cli, Command, ConfigAction, ConfigArgs, SearchArgs};
use crate::config::Config;
use crate::db::{Db, queries};
use crate::util;

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        None => search(SearchArgs {
            query: None,
            cmd_only: false,
        }),
        Some(Command::Search(args)) => search(args),
        Some(Command::Shell(args)) => shell::run(args),
        Some(Command::Proxy(args)) => proxy::run(args),
        Some(Command::Init(args)) => init::run(args),
        Some(Command::Setup(args)) => setup::run(args),
        Some(Command::Record(args)) => record::run(args),
        Some(Command::Import(args)) => import::run(args),
        Some(Command::Doctor) => doctor::run(),
        Some(Command::Prune) => prune(),
        Some(Command::Update(args)) => update::run(args),
        Some(Command::Config(args)) => config(args),
        Some(Command::Uuid) => {
            println!("{}", new_id());
            Ok(())
        }
    }
}

/// Generate a new sortable identifier (ULID).
pub fn new_id() -> String {
    ulid::Ulid::generate().to_string()
}

fn config(args: ConfigArgs) -> Result<()> {
    match args.action {
        ConfigAction::Path => {
            println!("{}", Config::config_path().display());
        }
        ConfigAction::Show => {
            let cfg = Config::load()?;
            print!("{}", toml::to_string_pretty(&cfg)?);
        }
        ConfigAction::Default => {
            let cfg = Config::default();
            print!("{}", toml::to_string_pretty(&cfg)?);
        }
    }
    Ok(())
}

fn prune() -> Result<()> {
    let cfg = Config::load()?;
    let db = Db::open(&cfg.general.db_path)?;
    let affected = queries::prune(&db.conn, cfg.retention.retention_days, util::now_ns())?;
    println!(
        "pruned output from {affected} block(s) older than {} day(s)",
        cfg.retention.retention_days
    );
    Ok(())
}

fn search(args: SearchArgs) -> Result<()> {
    let config = Config::load()?;
    let update_check = update::startup_check();
    let code = crate::tui::run(
        args,
        config,
        update_check.initial_notice,
        update_check.refreshed_notice,
    )?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}
