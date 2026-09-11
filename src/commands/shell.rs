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
    let login = resolve_login(&args, &config);

    let proxy_disabled = args.no_proxy
        || std::env::var("RECALL_PROXY").is_ok_and(|v| v == "0")
        || !std::io::stdin().is_terminal()
        || !std::io::stdout().is_terminal();

    if proxy_disabled {
        return run_plain(&shell);
    }

    match proxy::run(Arc::new(config), shell.clone(), login) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            util::eprintln_flush(&format!(
                "recall: proxy failed to start ({err}); falling back to a plain shell"
            ));
            run_plain(&shell)
        }
    }
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

fn resolve_login(args: &ShellArgs, config: &Config) -> bool {
    if args.no_login {
        false
    } else if args.login {
        true
    } else {
        config.proxy.login_shell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(login: bool, no_login: bool) -> ShellArgs {
        ShellArgs {
            no_proxy: false,
            login,
            no_login,
            shell: None,
        }
    }

    #[test]
    fn login_flag_overrides_config() {
        let mut config = Config::default();
        config.proxy.login_shell = false;

        assert!(resolve_login(&args(true, false), &config));
    }

    #[test]
    fn no_login_flag_overrides_config() {
        let mut config = Config::default();
        config.proxy.login_shell = true;

        assert!(!resolve_login(&args(false, true), &config));
    }

    #[test]
    fn falls_back_to_config_when_no_flag() {
        let mut config = Config::default();
        config.proxy.login_shell = true;
        assert!(resolve_login(&args(false, false), &config));

        config.proxy.login_shell = false;
        assert!(!resolve_login(&args(false, false), &config));
    }
}
