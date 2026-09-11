use std::sync::Arc;

use anyhow::Result;

use crate::capture::proxy;
use crate::cli::ProxyArgs;
use crate::config::Config;
use crate::util;

pub fn run(args: ProxyArgs) -> Result<()> {
    let config = Config::load()?;
    let shell = args
        .shell
        .clone()
        .or_else(|| (!config.proxy.shell.is_empty()).then(|| config.proxy.shell.clone()))
        .unwrap_or_else(util::login_shell);
    let login = config.proxy.login_shell;

    let code = proxy::run(Arc::new(config), shell, login)?;
    std::process::exit(code);
}
