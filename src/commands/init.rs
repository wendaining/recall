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

#[cfg(test)]
mod tests {
    #[test]
    fn macos_zsh_integration_supports_native_option_r() {
        let script = include_str!("../../shell/recall.zsh");
        assert!(script.contains("$OSTYPE == darwin*"));
        assert!(script.contains("</dev/tty"));
        assert!(script.contains("bindkey '®' _recall_search"));
    }
}
