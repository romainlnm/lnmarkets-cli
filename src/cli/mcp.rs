use clap::Args;

/// Service groups that can be exposed via MCP
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceGroup {
    /// Market data (public, read-only): get_ticker
    Market,
    /// Account info (authenticated, read-only): get_balance, list_trades
    Account,
    /// Trading operations (dangerous): open_trade, close_trade, update_stoploss, update_takeprofit, add_margin
    Trade,
    /// Funding operations (dangerous): deposit, withdraw
    Funding,
}

impl ServiceGroup {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "market" => Some(Self::Market),
            "account" => Some(Self::Account),
            "trade" => Some(Self::Trade),
            "funding" => Some(Self::Funding),
            _ => None,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::Market, Self::Account, Self::Trade, Self::Funding]
    }

    /// Default services (read-only)
    pub fn default_services() -> Vec<Self> {
        vec![Self::Market, Self::Account]
    }

    /// Services that contain dangerous operations
    pub fn is_dangerous(&self) -> bool {
        matches!(self, Self::Trade | Self::Funding)
    }
}

/// Parse service groups from comma-separated string
pub fn parse_services(s: &str) -> Vec<ServiceGroup> {
    if s.to_lowercase() == "all" {
        return ServiceGroup::all();
    }

    s.split(',')
        .filter_map(|part| ServiceGroup::from_str(part.trim()))
        .collect()
}

#[derive(Args)]
pub struct McpArgs {
    /// Service groups to expose (comma-separated or 'all')
    /// Available: market, account, trade, funding
    /// Default: market,account (read-only)
    #[arg(short = 's', long = "services", default_value = "market,account")]
    pub services: String,

    /// Allow dangerous operations without per-call confirmation.
    /// By default, tools like open_trade, close_trade, deposit, withdraw
    /// require an 'acknowledged=true' parameter. This flag removes that requirement.
    #[arg(long = "allow-dangerous")]
    pub allow_dangerous: bool,
}
