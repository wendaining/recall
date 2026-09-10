use serde::{Deserialize, Serialize};

/// Control messages emitted by the shell integration as private OSC 9999
/// payloads and parsed by the proxy.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Request {
    /// A command is about to run; begin capturing output.
    Start {
        id: String,
        command: String,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        atuin_id: Option<String>,
        #[serde(default)]
        started_at: Option<i64>,
    },
    /// The command finished; stop capturing and persist.
    End {
        id: String,
        #[serde(default)]
        exit: Option<i32>,
        #[serde(default)]
        duration_ns: Option<i64>,
    },
}
