use thiserror::Error;

#[derive(Error, Debug)]
pub enum SnapcastError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Not connected")]
    NotConnected,
}

pub type SnapcastResult<T> = Result<T, SnapcastError>;
