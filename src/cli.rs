use clap::{Args, Parser, Subcommand, ValueEnum};

/// Shell command and output history viewer.
#[derive(Debug, Parser)]
#[command(name = "recall", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Browse history in the TUI (default when no subcommand is given).
    Search(SearchArgs),
    /// Run an interactive shell under the output-capturing proxy.
    Shell(ShellArgs),
    /// Low-level proxy entry point (spawns the shell itself).
    Proxy(ProxyArgs),
    /// Print shell integration code: `eval "$(recall init zsh)"`.
    Init(InitArgs),
    /// Configure automatic output capture and shell integration.
    Setup(SetupArgs),
    /// Record metadata only, without captured output.
    Record(RecordArgs),
    /// Import metadata from another history source.
    Import(ImportArgs),
    /// Diagnose configuration, shell setup, databases and clipboard backends.
    Doctor,
    /// Drop stored output older than the retention window.
    Prune,
    /// Download and install the latest stable release.
    Update(UpdateArgs),
    /// Show or locate the configuration file.
    Config(ConfigArgs),
    /// Print a fresh recall id.
    Uuid,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Initial search query.
    pub query: Option<String>,
    /// Print the selected command to stdout instead of copying it.
    #[arg(long)]
    pub cmd_only: bool,
}

#[derive(Debug, Args)]
pub struct ShellArgs {
    /// Disable the proxy and run a plain shell.
    #[arg(long)]
    pub no_proxy: bool,
    /// Spawn the shell as a login shell (`-l`).
    #[arg(long, overrides_with = "no_login")]
    pub login: bool,
    /// Spawn the shell as a non-login shell, overriding the config.
    #[arg(long, overrides_with = "login")]
    pub no_login: bool,
    /// Shell binary to run (defaults to the login shell).
    #[arg(long)]
    pub shell: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProxyArgs {
    /// Shell binary to spawn (defaults to the login shell).
    #[arg(long)]
    pub shell: Option<String>,
    /// Print OSC 133 debug highlighting (accepted for compatibility).
    #[arg(long, hide = true)]
    pub debug_osc133: bool,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Shell to emit integration for.
    #[arg(value_parser = crate::shell::parse_integration_name)]
    pub shell: String,
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Shell to configure (defaults to the login shell).
    #[arg(value_parser = crate::shell::parse_integration_name)]
    pub shell: Option<String>,
    /// Configure automatic capture or install hooks only.
    #[arg(long, value_enum, default_value_t = SetupMode::Auto)]
    pub mode: SetupMode,
    /// Override the shell startup file to update.
    #[arg(long)]
    pub profile: Option<std::path::PathBuf>,
    /// Remove setup managed by recall from the startup file.
    #[arg(long)]
    pub remove: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum SetupMode {
    #[default]
    Auto,
    Hooks,
}

#[derive(Debug, Args)]
pub struct RecordArgs {
    #[arg(long)]
    pub command: String,
    #[arg(long)]
    pub exit: Option<i32>,
    #[arg(long)]
    pub duration_ns: Option<i64>,
    #[arg(long)]
    pub cwd: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    /// Shell name recorded with the block.
    #[arg(long)]
    pub shell: Option<String>,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    #[command(subcommand)]
    pub source: ImportSource,
}

#[derive(Debug, Subcommand)]
pub enum ImportSource {
    /// Backfill metadata from atuin's history database.
    Atuin {
        /// History database path (defaults to atuin's standard data directory).
        #[arg(long)]
        path: Option<std::path::PathBuf>,
        /// Only import commands newer than this many days (0 = all).
        #[arg(long, default_value_t = 0)]
        days: u32,
    },
    /// Import commands from a standard shell history file.
    History {
        /// History format to parse.
        #[arg(value_parser = crate::shell::parse_history_name)]
        shell: String,
        /// History file path (defaults to the shell's standard location).
        #[arg(long)]
        path: Option<std::path::PathBuf>,
    },
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Check for an update without downloading it.
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the configuration file path.
    Path,
    /// Print the effective configuration.
    Show,
    /// Print a commented default configuration.
    Default,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_defaults_to_automatic_capture() {
        let cli = Cli::try_parse_from(["recall", "setup", "zsh"]).unwrap();
        let Some(Command::Setup(args)) = cli.command else {
            panic!("expected setup command");
        };
        assert_eq!(args.mode, SetupMode::Auto);
        assert!(!args.remove);
    }

    #[test]
    fn setup_accepts_hooks_and_custom_profile() {
        let cli = Cli::try_parse_from([
            "recall",
            "setup",
            "pwsh",
            "--mode",
            "hooks",
            "--profile",
            "profile.ps1",
        ])
        .unwrap();
        let Some(Command::Setup(args)) = cli.command else {
            panic!("expected setup command");
        };
        assert_eq!(args.mode, SetupMode::Hooks);
        assert_eq!(
            args.profile.unwrap(),
            std::path::PathBuf::from("profile.ps1")
        );
    }
}
