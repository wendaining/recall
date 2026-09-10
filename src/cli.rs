use clap::{Args, Parser, Subcommand};

/// Warp-block style shell history viewer, complementary to atuin.
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
    /// Record metadata only, without captured output.
    Record(RecordArgs),
    /// Import metadata from another history source.
    Import(ImportArgs),
    /// Diagnose configuration, databases and clipboard backends.
    Doctor,
    /// Drop stored output older than the retention window.
    Prune,
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
    #[arg(value_parser = ["zsh", "bash", "fish"])]
    pub shell: String,
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
    pub atuin_id: Option<String>,
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
        /// Only import commands newer than this many days (0 = all).
        #[arg(long, default_value_t = 0)]
        days: u32,
    },
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
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
