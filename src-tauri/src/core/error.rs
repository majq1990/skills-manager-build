use serde::Serialize;
use std::fmt;

/// Structured error type for Tauri commands.
///
/// Serialized as `{"kind": "Database", "message": "..."}` so the frontend
/// can branch on `kind` while still showing a human-readable `message`.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Database,
    Io,
    Network,
    Git,
    NotFound,
    InvalidInput,
    Cancelled,
    Internal,
    Unauthorized,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl AppError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::NotFound,
            message: msg.into(),
        }
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::InvalidInput,
            message: msg.into(),
        }
    }

    #[allow(dead_code)]
    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Cancelled,
            message: msg.into(),
        }
    }

    /// Convert an `anyhow::Error` originating from database operations.
    pub fn db(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Database,
            message: e.to_string(),
        }
    }

    /// Convert an `anyhow::Error` originating from git operations.
    pub fn git(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Git,
            message: e.to_string(),
        }
    }

    /// Convert a git operation error, detecting cancellation from the message.
    pub fn git_or_cancelled(e: impl fmt::Display) -> Self {
        let message = e.to_string();
        let lower = message.to_ascii_lowercase();
        if lower.contains("cancelled") || lower.contains("canceled") {
            Self {
                kind: ErrorKind::Cancelled,
                message,
            }
        } else {
            Self {
                kind: ErrorKind::Git,
                message,
            }
        }
    }

    /// Convert an `anyhow::Error` originating from network operations.
    pub fn network(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Network,
            message: e.to_string(),
        }
    }

    /// Convert an `anyhow::Error` originating from IO operations.
    pub fn io(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: e.to_string(),
        }
    }

    pub fn internal(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: e.to_string(),
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Unauthorized,
            message: msg.into(),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: e.to_string(),
        }
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: e.to_string(),
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_or_cancelled_detects_cancelled() {
        let err = AppError::git_or_cancelled("Installation cancelled by user");
        assert!(matches!(err.kind, ErrorKind::Cancelled));
    }

    #[test]
    fn git_or_cancelled_detects_canceled_american_spelling() {
        let err = AppError::git_or_cancelled("Operation was canceled");
        assert!(matches!(err.kind, ErrorKind::Cancelled));
    }

    #[test]
    fn git_or_cancelled_regular_git_error() {
        let err = AppError::git_or_cancelled("Failed to push to remote");
        assert!(matches!(err.kind, ErrorKind::Git));
    }

    #[test]
    fn git_or_cancelled_case_insensitive() {
        let err = AppError::git_or_cancelled("CANCELLED by system");
        assert!(matches!(err.kind, ErrorKind::Cancelled));
    }

    #[test]
    fn constructors_set_correct_kinds() {
        assert!(matches!(AppError::not_found("x").kind, ErrorKind::NotFound));
        assert!(matches!(
            AppError::invalid_input("x").kind,
            ErrorKind::InvalidInput
        ));
        assert!(matches!(
            AppError::cancelled("x").kind,
            ErrorKind::Cancelled
        ));
        assert!(matches!(AppError::db("x").kind, ErrorKind::Database));
        assert!(matches!(AppError::git("x").kind, ErrorKind::Git));
        assert!(matches!(AppError::network("x").kind, ErrorKind::Network));
        assert!(matches!(AppError::io("x").kind, ErrorKind::Io));
        assert!(matches!(AppError::internal("x").kind, ErrorKind::Internal));
    }

    #[test]
    fn display_shows_message() {
        let err = AppError::not_found("Skill not found");
        assert_eq!(format!("{}", err), "Skill not found");
    }

    #[test]
    fn serializes_to_json_with_kind_and_message() {
        let err = AppError::not_found("missing");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "not_found");
        assert_eq!(json["message"], "missing");
    }

    #[test]
    fn error_kind_serializes_snake_case() {
        let err = AppError::invalid_input("bad");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "invalid_input");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let app_err: AppError = io_err.into();
        assert!(matches!(app_err.kind, ErrorKind::Io));
        assert!(app_err.message.contains("file gone"));
    }
}
