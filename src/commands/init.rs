use anyhow::Result;

use crate::cli::InitArgs;
use crate::config::Config;
use crate::shell::Shell;

const SEARCH_KEY_PLACEHOLDER: &str = "@RECALL_SEARCH_KEY@";

pub fn run(args: InitArgs) -> Result<()> {
    let shell = Shell::from_name(&args.shell).expect("clap restricts the shell value");
    let script = shell
        .init_script()
        .expect("clap restricts init to integrated shells");
    let config = Config::load().unwrap_or_default();
    let key = shell.search_key(&config.ui.search_key);
    print!("{}", script.replace(SEARCH_KEY_PLACEHOLDER, &key));
    Ok(())
}
