use anyhow::Result;

use crate::cli::{ImportArgs, ImportSource};

mod atuin;
mod history;

pub fn run(args: ImportArgs) -> Result<()> {
    match args.source {
        ImportSource::Atuin { path, days } => atuin::run(path, days),
        ImportSource::History { shell, path } => history::run(shell, path),
    }
}
