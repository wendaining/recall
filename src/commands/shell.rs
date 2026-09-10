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
        return run_plain(&shell);
    }

    let code = proxy::run(Arc::new(config), shell)?;
    std::process::exit(code);
}

#[cfg(unix)]
fn run_plain(shell: &str) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(shell).exec();
    Err(anyhow!("failed to exec {shell}: {err}"))
}

#[cfg(not(unix))]
fn run_plain(shell: &str) -> Result<()> {
    let status = std::process::Command::new(shell).status()?;
    std::process::exit(status.code().unwrap_or(0));
}

fn resolve_shell(args: &ShellArgs, config: &Config) -> String {
    args.shell
        .clone()
        .or_else(|| (!config.proxy.shell.is_empty()).then(|| config.proxy.shell.clone()))
        .unwrap_or_else(util::login_shell)
}
