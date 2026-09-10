use anyhow::Result;

use crate::cli::InitArgs;

pub fn run(args: InitArgs) -> Result<()> {
    match args.shell.as_str() {
        "zsh" => print!("{}", include_str!("../../shell/recall.zsh")),
        _ => unreachable!("clap restricts the shell value"),
    }
    Ok(())
}
