use anyhow::Result;
use clap::Subcommand;
use std::io::{self, Write};

use crate::config::{Config, Credentials, OutputFormat};
use super::output::{print_success, print_error, print_info};

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Configure API credentials
    Login,

    /// Remove stored credentials
    Logout,

    /// Show current authentication status
    Status,

    /// Show where credentials are stored
    Whoami,
}

impl AuthCommands {
    pub async fn execute(&self, format: OutputFormat) -> Result<()> {
        match self {
            Self::Login => {
                print_info("Configure LN Markets API credentials");
                print_info("Get your API keys from: https://lnmarkets.com/user/api\n");

                let api_key = prompt("API Key")?;
                let api_secret = prompt("API Secret")?;
                let passphrase = prompt("Passphrase")?;

                let mut config = Config::load().unwrap_or_default();
                config.credentials = Credentials {
                    api_key: Some(api_key),
                    api_secret: Some(api_secret),
                    passphrase: Some(passphrase),
                };

                config.save()?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        println!(r#"{{"status":"configured"}}"#);
                    }
                    OutputFormat::Table => {
                        print_success("Credentials saved successfully");
                        print_info(&format!("Config location: {:?}", Config::config_path()?));
                    }
                }
            }

            Self::Logout => {
                let mut config = Config::load().unwrap_or_default();
                config.credentials = Credentials::default();
                config.save()?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        println!(r#"{{"status":"logged_out"}}"#);
                    }
                    OutputFormat::Table => {
                        print_success("Credentials removed");
                    }
                }
            }

            Self::Status => {
                let config = Config::load().unwrap_or_default();
                let has_creds = config.has_credentials();
                let has_env = std::env::var("LNM_API_KEY").is_ok();

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        println!(
                            r#"{{"authenticated":{},"source":"{}"}}"#,
                            has_creds || has_env,
                            if has_env { "environment" } else if has_creds { "config" } else { "none" }
                        );
                    }
                    OutputFormat::Table => {
                        if has_env {
                            print_success("Authenticated via environment variables");
                        } else if has_creds {
                            print_success("Authenticated via config file");
                        } else {
                            print_error("Not authenticated");
                            print_info("Run 'lnm auth login' to configure credentials");
                            print_info("Or set LNM_API_KEY, LNM_API_SECRET, LNM_API_PASSPHRASE environment variables");
                        }
                    }
                }
            }

            Self::Whoami => {
                let config = Config::load().unwrap_or_default();
                let creds = config.get_credentials();

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        let key_preview = creds.api_key
                            .as_ref()
                            .map(|k| format!("{}...", k.chars().take(8).collect::<String>()))
                            .unwrap_or_else(|| "none".to_string());

                        println!(
                            r#"{{"config_path":"{:?}","api_key_preview":"{}"}}"#,
                            Config::config_path()?,
                            key_preview
                        );
                    }
                    OutputFormat::Table => {
                        print_info(&format!("Config path: {:?}", Config::config_path()?));

                        if let Some(key) = &creds.api_key {
                            let preview: String = key.chars().take(8).collect();
                            print_info(&format!("API Key: {}...", preview));
                        } else {
                            print_info("API Key: not configured");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

fn prompt(label: &str) -> Result<String> {
    print!("{}: ", label);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_string())
}
