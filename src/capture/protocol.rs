use serde::{Deserialize, Serialize};

/// Messages sent by the shell integration to the proxy over the control socket.
/// One JSON object per line.
#[derive(Debug, Deserialize)]
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

#[derive(Debug, Serialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
        }
    }
}
