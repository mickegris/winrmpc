use thiserror::Error;

#[derive(Error, Debug)]
pub enum MpdError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Server error [{code}]: {message}")]
    Server { code: u32, message: String },

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not connected")]
    NotConnected,

    #[error("Timeout")]
    Timeout,
}

impl MpdError {
    /// Whether this error means the *connection* is no longer trustworthy,
    /// as opposed to MPD simply refusing a command.
    ///
    /// `Server` (an `ACK`) is a normal, well-framed reply — the socket is
    /// fine and must be kept. Everything else means the byte stream is not
    /// where we think it is: an IO/UTF-8 failure, a half-consumed binary
    /// payload, or a header we couldn't parse and therefore didn't finish
    /// reading. Once that happens the connection is permanently desynced —
    /// every later `read_line` picks up the tail of someone else's response,
    /// which is how a single bad album-art read turned into an app that
    /// logged `stream did not contain valid UTF-8` forever and answered
    /// `currentsong` with an ACK addressed to `{albumart}`.
    pub fn is_connection_fatal(&self) -> bool {
        !matches!(self, MpdError::Server { .. } | MpdError::NotConnected)
    }
}

pub type MpdResult<T> = Result<T, MpdError>;
