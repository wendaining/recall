use anyhow::Result;

use crate::cli::InitArgs;

pub fn run(args: InitArgs) -> Result<()> {
    let script = match args.shell.as_str() {
        "zsh" => include_str!("../../shell/recall.zsh"),
        "bash" => include_str!("../../shell/recall.bash"),
        "fish" => include_str!("../../shell/recall.fish"),
        _ => unreachable!("clap restricts the shell value"),
    };
    print!("{script}");
    Ok(())
}
