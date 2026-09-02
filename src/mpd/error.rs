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

    /// The server refused our `password`. Distinct from `Server` because it
    /// is the one ACK that must abort the connect rather than be reported as
    /// a failed command: an unauthenticated connection answers every later
    /// command with a permission ACK, which is not connection-fatal, so it
    /// would never be dropped and never retried.
    #[error("Authentication failed: {0}")]
    Auth(String),

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
        // `Auth` is grouped with the two non-fatal arms for the same reason
        // they are there: the socket itself is fine, MPD simply refused. It
        // can only be produced by `connect`, which discards the connection
        // itself, so it never actually reaches this test through `cmd` —
        // classifying it as "the stream is desynced" would be a lie.
        !matches!(
            self,
            MpdError::Server { .. } | MpdError::NotConnected | MpdError::Auth(_)
        )
    }
}

pub type MpdResult<T> = Result<T, MpdError>;
