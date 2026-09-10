use std::io::IsTerminal;
use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::capture::proxy;
use crate::cli::ShellArgs;
use crate::config::Config;
use crate::util;

pub fn run(args: ShellArgs) -> Result<()> {
    let config = Config::load()?;
    let shell = resolve_shell(&args, &config);

    let proxy_disabled = args.no_proxy
        || std::env::var("RECALL_PROXY").is_ok_and(|v| v == "0")
        || !std::io::stdin().is_terminal()
        || !std::io::stdout().is_terminal();

    if proxy_disabled {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&shell).exec();
        return Err(anyhow!("failed to exec {shell}: {err}"));
    }

    let code = proxy::run(Arc::new(config), shell)?;
    std::process::exit(code);
}

fn resolve_shell(args: &ShellArgs, config: &Config) -> String {
    args.shell
        .clone()
        .or_else(|| (!config.proxy.shell.is_empty()).then(|| config.proxy.shell.clone()))
        .unwrap_or_else(util::login_shell)
}
