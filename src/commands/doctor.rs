use anyhow::Result;

use super::setup::{self, ManagedSetup};
use crate::config::Config;
use crate::db::{self, Db, queries, schema};
use crate::shell::Shell;
use crate::util;

pub fn run() -> Result<()> {
    let cfg = Config::load()?;

    println!("recall {}", env!("CARGO_PKG_VERSION"));
    println!("config:  {}", Config::config_path().display());
    println!("shell:   {}", util::login_shell());
    println!(
        "hostname: {}",
        util::resolved_hostname(&cfg).unwrap_or_else(|| "?".into())
    );
    println!("TERM:    {}", std::env::var("TERM").unwrap_or_default());
    println!();

    check_recall_db(&cfg)?;
    check_clipboard(&cfg);
    check_shell_integration(&cfg);
    check_macos_option_key();

    Ok(())
}

fn check_recall_db(cfg: &Config) -> Result<()> {
    let path = &cfg.general.db_path;
    println!("[recall db] {}", path.display());
    println!("  error log: {}", db::error_log_path(path).display());
    match Db::open(path) {
        Ok(db) => {
            let count = queries::count(&db.conn).unwrap_or(-1);
            let version: i64 = db
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap_or(-1);
            println!(
                "  ok: {count} block(s), schema v{version} (expected v{})",
                schema::SCHEMA_VERSION
            );
        }
        Err(err) => println!("  ERROR: {err}"),
    }
    Ok(())
}

fn check_clipboard(cfg: &Config) {
    println!("[clipboard] configured backend: {}", cfg.clipboard.backend);
    #[cfg(unix)]
    {
        for tool in ["wl-copy", "xclip", "xsel", "pbcopy"] {
            println!(
                "  {tool:<8} {}",
                if util::command_exists(tool) {
                    "found"
                } else {
                    "missing"
                }
            );
        }
        println!("  arboard   built-in (native)");
        println!("  osc52     always available inside a supporting terminal");
        println!(
            "  session:  {} / {}",
            std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".into()),
            std::env::var("WAYLAND_DISPLAY")
                .or_else(|_| std::env::var("DISPLAY"))
                .unwrap_or_else(|_| "no display".into())
        );
    }
    #[cfg(windows)]
    {
        println!(
            "  clip      {}",
            if util::command_exists("clip") {
                "found"
            } else {
                "missing"
            }
        );
        println!("  arboard   built-in (native)");
        println!("  osc52     always available inside a supporting terminal");
        println!(
            "  host:     {}",
            if std::env::var_os("WT_SESSION").is_some() {
                "Windows Terminal"
            } else {
                "console host"
            }
        );
    }
}

fn check_shell_integration(cfg: &Config) {
    println!("[shell integration]");
    match std::env::current_exe() {
        Ok(exe) => println!("  binary:  {}", exe.display()),
        Err(err) => println!("  binary:  ERROR: {err}"),
    }
    println!(
        "  on PATH: {}",
        if util::command_exists("recall") {
            "yes"
        } else {
            "no (the proxied shell cannot run `recall`)"
        }
    );

    let shell = util::login_shell();
    let mut setup_state = ManagedSetup::None;
    let mut init_count = 0;
    match Shell::from_command(&shell).and_then(Shell::rc_path) {
        Some(path) => match setup::inspect_profile(&path) {
            Ok(inspection) => {
                setup_state = inspection.setup;
                init_count = inspection.init_count;
                println!("  rc file: {}", path.display());
            }
            Err(err) => println!("  rc file: {} (ERROR: {err})", path.display()),
        },
        None => println!("  rc file: unknown shell {shell}"),
    }

    println!(
        "  setup:   {}",
        match setup_state {
            ManagedSetup::Auto => "automatic output capture (managed)",
            ManagedSetup::Hooks => "hooks only (managed)",
            ManagedSetup::LegacyAuto => "automatic output capture (legacy installer setup)",
            ManagedSetup::LegacyHooks => "hooks only (legacy installer setup)",
            ManagedSetup::UnmanagedHooks => "hooks only (unmanaged recall init)",
            ManagedSetup::None => "not configured",
            ManagedSetup::Invalid => "ERROR: malformed recall setup markers",
        }
    );
    println!(
        "  hooks:   {}",
        match init_count {
            0 => "not configured".to_string(),
            1 => {
                if std::env::var_os("RECALL_HOOKS_ACTIVE").is_some() {
                    "configured and active".to_string()
                } else {
                    "configured; not active in this shell".to_string()
                }
            }
            count => format!("WARNING: {count} active recall init lines found"),
        }
    );
    if init_count > 1 {
        println!("  hint:    remove duplicate active `recall init` lines from the startup file");
    }
    if matches!(
        setup_state,
        ManagedSetup::LegacyAuto | ManagedSetup::LegacyHooks
    ) {
        println!("  hint:    run `recall setup` to migrate the legacy installer setup");
    }

    let proxy_active = std::env::var_os("RECALL_PROXY_ACTIVE").is_some();
    println!(
        "  proxy:   {}",
        if proxy_active {
            "active"
        } else if std::env::var("RECALL_PROXY").is_ok_and(|value| value == "0") {
            "disabled by RECALL_PROXY=0"
        } else {
            "not active"
        }
    );
    if !proxy_active {
        match setup_state {
            ManagedSetup::Auto | ManagedSetup::LegacyAuto => {
                if std::env::var("RECALL_PROXY").is_ok_and(|value| value == "0") {
                    println!(
                        "  hint:    unset RECALL_PROXY and open a new terminal to capture output"
                    );
                } else {
                    println!("  hint:    open a new terminal to activate automatic output capture");
                }
            }
            ManagedSetup::Hooks | ManagedSetup::LegacyHooks | ManagedSetup::UnmanagedHooks => {
                println!(
                    "  hint:    hooks record metadata only; run `recall shell` to capture output"
                );
            }
            ManagedSetup::None => {
                println!("  hint:    run `recall setup` to enable automatic output capture");
            }
            ManagedSetup::Invalid => {
                println!("  hint:    repair the setup markers before running `recall setup` again");
            }
        }
    }
    println!(
        "  login:   {}",
        if cfg!(unix) {
            if cfg.proxy.login_shell {
                "enabled (-l)"
            } else {
                "disabled"
            }
        } else {
            "n/a on Windows"
        }
    );
}

#[cfg(target_os = "macos")]
fn check_macos_option_key() {
    println!("[macOS Option key]");
    println!("  Alt+R sends Option+R. If it inserts '®', enable \"Use Option as Meta key\"");
    println!("  in your terminal (Terminal.app, iTerm2, Ghostty), or set [ui].search_key");
    println!("  to another key such as \"ctrl-x ctrl-r\".");
}

#[cfg(not(target_os = "macos"))]
fn check_macos_option_key() {}
