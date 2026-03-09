use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub credentials: Credentials,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_network")]
    pub network: Network,
    #[serde(default = "default_output")]
    pub output_format: OutputFormat,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            network: Network::Mainnet,
            output_format: OutputFormat::Table,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Mainnet,
    Testnet,
}

impl Network {
    pub fn base_url(&self) -> &'static str {
        match self {
            Network::Mainnet => "https://api.lnmarkets.com",
            Network::Testnet => "https://api.testnet4.lnmarkets.com",
        }
    }

    pub fn ws_url(&self) -> &'static str {
        match self {
            Network::Mainnet => "wss://api.lnmarkets.com",
            Network::Testnet => "wss://api.testnet4.lnmarkets.com",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    JsonPretty,
}

fn default_network() -> Network {
    Network::Mainnet
}

fn default_output() -> OutputFormat {
    OutputFormat::Table
}

impl Config {
    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .context("Could not determine config directory")?
            .join("lnmarkets");
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {:?}", path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| "Failed to parse config file")?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir()?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory {:?}", dir))?;

        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        Ok(())
    }

    /// Get credentials from config or environment variables
    pub fn get_credentials(&self) -> Credentials {
        Credentials {
            api_key: std::env::var("LNM_API_KEY")
                .ok()
                .or_else(|| self.credentials.api_key.clone()),
            api_secret: std::env::var("LNM_API_SECRET")
                .ok()
                .or_else(|| self.credentials.api_secret.clone()),
            passphrase: std::env::var("LNM_API_PASSPHRASE")
                .ok()
                .or_else(|| self.credentials.passphrase.clone()),
        }
    }

    pub fn has_credentials(&self) -> bool {
        let creds = self.get_credentials();
        creds.api_key.is_some() && creds.api_secret.is_some() && creds.passphrase.is_some()
    }
}
