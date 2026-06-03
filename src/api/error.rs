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
