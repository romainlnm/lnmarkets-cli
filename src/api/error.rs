use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize)]
pub struct ApiError {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(msg) = &self.message {
            write!(f, "{}", msg)?;
        } else if let Some(err) = &self.error {
            write!(f, "{}", err)?;
        } else {
            write!(f, "Unknown API error")?;
        }

        if let Some(code) = &self.code {
            write!(f, " (code: {})", code)?;
        }

        Ok(())
    }
}

impl std::error::Error for ApiError {}

/// Error categories for consistent handling
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorCategory {
    /// Authentication failed
    Auth,
    /// Rate limit exceeded
    RateLimit,
    /// Invalid request parameters
    Validation,
    /// Resource not found
    NotFound,
    /// Network/connection error
    Network,
    /// Server error
    Server,
    /// Unknown error
    Unknown,
}

impl ErrorCategory {
    pub fn from_status(status: u16) -> Self {
        match status {
            401 | 403 => Self::Auth,
            429 => Self::RateLimit,
            400 | 422 => Self::Validation,
            404 => Self::NotFound,
            500..=599 => Self::Server,
            _ => Self::Unknown,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Auth => 2,
            Self::RateLimit => 3,
            Self::Validation => 4,
            Self::NotFound => 5,
            Self::Network => 6,
            Self::Server => 7,
            Self::Unknown => 1,
        }
    }
}
