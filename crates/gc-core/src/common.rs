//! Minimal application primitives used by the mail modules.
pub type Millis = i64;
pub fn now_millis() -> Millis {
    chrono::Utc::now().timestamp_millis()
}
pub type AppResult<T> = Result<T, AppError>;
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub message: String,
}
impl AppError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
    pub fn unhandled(error: impl std::fmt::Display) -> Self {
        Self::internal(error.to_string())
    }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::unhandled(e)
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::unhandled(e)
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::unhandled(e)
    }
}
