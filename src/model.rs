use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Classification of a captured block. Drives how the TUI presents output and
/// whether the output is meaningful at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    /// Normal command with captured text output.
    #[default]
    Normal,
    /// Command produced no visible output.
    Empty,
    /// Full-screen / alt-screen program (TUI). Output is intentionally skipped.
    Interactive,
    /// Output was mostly binary / non-printable; only a summary is kept.
    Binary,
    /// Output was redirected away from the terminal, so it could not be seen.
    Redirected,
    /// Capture was not active (proxy disabled or failed).
    Unavailable,
    /// Output matched a secret filter and was not stored.
    Filtered,
}

impl BlockKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            BlockKind::Normal => "normal",
            BlockKind::Empty => "empty",
            BlockKind::Interactive => "interactive",
            BlockKind::Binary => "binary",
            BlockKind::Redirected => "redirected",
            BlockKind::Unavailable => "unavailable",
            BlockKind::Filtered => "filtered",
        }
    }
}

impl fmt::Display for BlockKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BlockKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "normal" => BlockKind::Normal,
            "empty" => BlockKind::Empty,
            "interactive" => BlockKind::Interactive,
            "binary" => BlockKind::Binary,
            "redirected" => BlockKind::Redirected,
            "unavailable" => BlockKind::Unavailable,
            "filtered" => BlockKind::Filtered,
            _ => return Err(()),
        })
    }
}

/// A single recorded command execution.
///
/// `output` holds the *uncompressed* plain-text (ANSI-stripped) output while in
/// memory. It is zstd-compressed before being written to SQLite.
#[derive(Debug, Clone, Default)]
pub struct Block {
    pub id: String,
    pub atuin_id: Option<String>,
    pub session: Option<String>,
    pub hostname: Option<String>,
    pub shell: Option<String>,
    pub command: String,
    pub cwd: Option<String>,
    /// Start time in nanoseconds since the Unix epoch.
    pub started_at: i64,
    pub duration_ns: Option<i64>,
    pub exit_code: Option<i32>,
    pub output: Option<Vec<u8>>,
    pub output_codec: Option<String>,
    pub output_bytes: i64,
    pub output_lines: i64,
    pub output_truncated: bool,
    pub kind: BlockKind,
    pub created_at: i64,
}

impl Block {
    /// Whether this block has stored output worth showing.
    pub fn has_output(&self) -> bool {
        self.output_bytes > 0 && self.kind == BlockKind::Normal
    }
}
