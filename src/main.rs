mod capture;
mod cli;
mod commands;
mod config;
mod db;
mod model;
mod util;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    commands::run(cli)
}
