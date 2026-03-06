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

pub type MpdResult<T> = Result<T, MpdError>;
