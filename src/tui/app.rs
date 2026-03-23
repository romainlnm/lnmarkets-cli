use super::popup::{Notification, Popup};
use crate::api::LnmClient;
use crate::models::funding::{Deposit, Withdrawal};
use crate::models::futures::{MarginType, Trade};
use crate::models::market::Ticker;
use crate::models::user::User;
use crate::recap::MarketRecap;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use reqwest::Method;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Positions,
    Orders,
    History,
    Funding,
    Recap,
}
impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Dashboard,
        Tab::Positions,
        Tab::Orders,
        Tab::History,
        Tab::Funding,
        Tab::Recap,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Positions => "Positions",
            Tab::Orders => "Orders",
            Tab::History => "History",
            Tab::Funding => "Funding",
            Tab::Recap => "Recap",
        }
    }
    pub fn index(&self) -> usize {
        match self {
            Tab::Dashboard => 0,
            Tab::Positions => 1,
            Tab::Orders => 2,
            Tab::History => 3,
            Tab::Funding => 4,
            Tab::Recap => 5,
        }
    }
}

pub struct App {
    pub active_tab: Tab,
    pub selected_row: usize,
    pub ticker: Option<Ticker>,
    pub user: Option<User>,
    pub positions: Vec<Trade>,
    pub orders: Vec<Trade>,
    pub closed_trades: Vec<Trade>,
    pub deposits: Vec<Deposit>,
    pub withdrawals: Vec<Withdrawal>,
    pub price_history: Vec<f64>,
    pub recap: Option<MarketRecap>,
    pub daemon_status: Option<String>,
    pub popup: Option<Popup>,
    pub notifications: Vec<Notification>,
    pub last_refresh: String,
    pub error: Option<String>,
    pub authenticated: bool,
    pub refresh_secs: u64,
    pub client: Option<LnmClient>,
    pub dark_theme: bool,
    pub use_testnet: bool,
}

impl App {
    pub fn new(refresh_secs: u64, client: Option<LnmClient>) -> Self {
        Self {
            active_tab: Tab::Dashboard,
            selected_row: 0,
            ticker: None,
            user: None,
            positions: Vec::new(),
            orders: Vec::new(),
            closed_trades: Vec::new(),
            deposits: Vec::new(),
            withdrawals: Vec::new(),
            price_history: Vec::new(),
            recap: None,
            daemon_status: None,
            popup: None,
            notifications: Vec::new(),
            last_refresh: String::new(),
            error: None,
            authenticated: client.is_some(),
            refresh_secs,
            client,
            dark_theme: true,
            use_testnet: false,
        }
    }
    pub fn notify(&mut self, n: Notification) {
        self.notifications.push(n);
    }
    pub fn tick_notifications(&mut self) {
        self.notifications.retain(|n| !n.is_expired());
    }
    pub fn balance(&self) -> Option<i64> {
        self.user.as_ref().and_then(|u| u.balance)
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.popup.is_some() {
            self.handle_popup_key(key).await;
            return false;
        }
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Esc => return true,
            KeyCode::Char('?') | KeyCode::Char('h') => {
                self.popup = Some(Popup::help());
            }
            KeyCode::Tab | KeyCode::Right => {
                let i = self.active_tab.index();
                self.active_tab = Tab::ALL[(i + 1) % Tab::ALL.len()];
                self.selected_row = 0;
            }
            KeyCode::BackTab | KeyCode::Left => {
                let i = self.active_tab.index();
                self.active_tab = Tab::ALL[(i + Tab::ALL.len() - 1) % Tab::ALL.len()];
                self.selected_row = 0;
            }
            KeyCode::Char('1') => {
                self.active_tab = Tab::Dashboard;
                self.selected_row = 0;
            }
            KeyCode::Char('2') => {
                self.active_tab = Tab::Positions;
                self.selected_row = 0;
            }
            KeyCode::Char('3') => {
                self.active_tab = Tab::Orders;
                self.selected_row = 0;
            }
            KeyCode::Char('4') => {
                self.active_tab = Tab::History;
                self.selected_row = 0;
            }
            KeyCode::Char('5') => {
                self.active_tab = Tab::Funding;
                self.selected_row = 0;
            }
            KeyCode::Char('6') => {
                self.active_tab = Tab::Recap;
                self.selected_row = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let m = self.row_count();
                if m > 0 {
                    self.selected_row = (self.selected_row + 1).min(m - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_row = self.selected_row.saturating_sub(1);
            }
            KeyCode::Char('T') => {
                self.dark_theme = !self.dark_theme;
                self.notify(Notification::info(if self.dark_theme {
                    "Dark"
                } else {
                    "Light"
                }));
            }
            KeyCode::Char('N') => {
                self.use_testnet = !self.use_testnet;
                self.notify(Notification::info(if self.use_testnet {
                    "Testnet"
                } else {
                    "Mainnet"
                }));
            }
            KeyCode::Char('o') if self.authenticated => {
                self.popup = Some(Popup::open_position());
            }
            KeyCode::Char('L') if !self.authenticated => {
                self.popup = Some(Popup::login());
            }
            KeyCode::Char('D') if self.authenticated => {
                self.popup = Some(Popup::launch_daemon());
            }
            KeyCode::Char('i') if self.active_tab == Tab::Dashboard => {
                if let Some(ref u) = self.user {
                    let mut l = vec![];
                    if let Some(ref un) = u.username {
                        l.push(("Username".into(), un.clone()));
                    }
                    if let Some(b) = u.balance {
                        l.push(("Balance".into(), format!("{} sats", b)));
                    }
                    if let Some(ref c) = u.created_at {
                        l.push(("Created".into(), c.clone()));
                    }
                    if let Some(ref lu) = u.last_update {
                        l.push(("Updated".into(), lu.clone()));
                    }
                    if let Some(ref ds) = self.daemon_status {
                        l.push(("Daemon".into(), ds.clone()));
                    }
                    l.push((
                        "Network".into(),
                        (if self.use_testnet {
                            "testnet"
                        } else {
                            "mainnet"
                        })
                        .into(),
                    ));
                    self.popup = Some(Popup::Detail {
                        title: "Account".into(),
                        lines: l,
                    });
                }
            }
            KeyCode::Char('u') if self.active_tab == Tab::Dashboard && self.authenticated => {
                self.popup = Some(Popup::update_account());
            }
            KeyCode::Char('e') => {
                let s = serde_json::json!({"balance":self.balance(),"positions":self.positions.len(),"orders":self.orders.len(),"index":self.ticker.as_ref().map(|t|t.index),"time":self.last_refresh});
                self.notify(Notification::info(format!("{}", s)));
            }
            KeyCode::Char('O') if self.authenticated => {
                self.popup = Some(Popup::confirm_logout());
            }
            // Positions
            KeyCode::Enter if self.active_tab == Tab::Positions => {
                if let Some(p) = self.positions.get(self.selected_row) {
                    self.popup = Some(Popup::position_detail(
                        &p.id,
                        &p.side,
                        p.quantity,
                        p.leverage,
                        p.entry_price,
                        p.margin,
                        p.pl,
                        p.stop_loss,
                        p.take_profit,
                    ));
                }
            }
            KeyCode::Char('c') if self.active_tab == Tab::Positions => {
                if let Some(p) = self.positions.get(self.selected_row) {
                    let (id, s, q, pl) =
                        (p.id.clone(), p.side.clone(), p.quantity, p.pl.unwrap_or(0));
                    self.popup = Some(Popup::confirm_close(&id, &s, q, pl));
                }
            }
            KeyCode::Char('C')
                if self.active_tab == Tab::Positions && !self.positions.is_empty() =>
            {
                self.popup = Some(Popup::confirm_close_all(self.positions.len()));
            }
            KeyCode::Char('s') if self.active_tab == Tab::Positions => {
                if let Some(p) = self.positions.get(self.selected_row) {
                    self.popup = Some(Popup::update_stoploss(&p.id, p.stop_loss));
                }
            }
            KeyCode::Char('t') if self.active_tab == Tab::Positions => {
                if let Some(p) = self.positions.get(self.selected_row) {
                    self.popup = Some(Popup::update_takeprofit(&p.id, p.take_profit));
                }
            }
            KeyCode::Char('m') if self.active_tab == Tab::Positions => {
                if let Some(p) = self.positions.get(self.selected_row) {
                    self.popup = Some(Popup::add_margin(&p.id));
                }
            }
            KeyCode::Char('$') if self.active_tab == Tab::Positions => {
                if let Some(p) = self.positions.get(self.selected_row) {
                    self.popup = Some(Popup::cash_in(&p.id));
                }
            }
            // Orders
            KeyCode::Char('x') if self.active_tab == Tab::Orders => {
                if let Some(o) = self.orders.get(self.selected_row) {
                    self.popup = Some(Popup::confirm_cancel(&o.id));
                }
            }
            KeyCode::Char('X') if self.active_tab == Tab::Orders && !self.orders.is_empty() => {
                self.popup = Some(Popup::confirm_cancel_all(self.orders.len()));
            }
            // Funding
            KeyCode::Char('d') if self.active_tab == Tab::Funding && self.authenticated => {
                self.popup = Some(Popup::deposit_lightning());
            }
            KeyCode::Char('w') if self.active_tab == Tab::Funding && self.authenticated => {
                self.popup = Some(Popup::withdraw_lightning());
            }
            KeyCode::Char('W') if self.active_tab == Tab::Funding && self.authenticated => {
                self.popup = Some(Popup::withdraw_onchain());
            }
            KeyCode::Char('a') if self.active_tab == Tab::Funding && self.authenticated => {
                self.popup = Some(Popup::new_btc_address());
            }
            _ => {}
        }
        false
    }
    async fn handle_popup_key(&mut self, key: KeyEvent) {
        let p = match self.popup.take() {
            Some(p) => p,
            _ => return,
        };
        match p {
            Popup::Confirm { action, .. } => {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    self.execute_confirm(action).await;
                }
            }
            Popup::Form {
                title,
                mut fields,
                mut active_field,
                action,
            } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    if active_field >= fields.len() - 1 {
                        self.execute_form(action, &fields).await;
                    } else {
                        active_field += 1;
                        self.popup = Some(Popup::Form {
                            title,
                            fields,
                            active_field,
                            action,
                        });
                    }
                }
                KeyCode::Tab | KeyCode::Down => {
                    active_field = (active_field + 1).min(fields.len() - 1);
                    self.popup = Some(Popup::Form {
                        title,
                        fields,
                        active_field,
                        action,
                    });
                }
                KeyCode::BackTab | KeyCode::Up => {
                    active_field = active_field.saturating_sub(1);
                    self.popup = Some(Popup::Form {
                        title,
                        fields,
                        active_field,
                        action,
                    });
                }
                KeyCode::Backspace => {
                    if let Some(f) = fields.get_mut(active_field) {
                        f.pop();
                    }
                    self.popup = Some(Popup::Form {
                        title,
                        fields,
                        active_field,
                        action,
                    });
                }
                KeyCode::Char(c) => {
                    if let Some(f) = fields.get_mut(active_field) {
                        f.push(c);
                    }
                    self.popup = Some(Popup::Form {
                        title,
                        fields,
                        active_field,
                        action,
                    });
                }
                _ => {
                    self.popup = Some(Popup::Form {
                        title,
                        fields,
                        active_field,
                        action,
                    });
                }
            },
            Popup::Detail { .. } | Popup::Help => {}
        }
    }
    fn row_count(&self) -> usize {
        match self.active_tab {
            Tab::Positions => self.positions.len(),
            Tab::Orders => self.orders.len(),
            Tab::History => self.closed_trades.len(),
            Tab::Funding => self.deposits.len() + self.withdrawals.len(),
            _ => 0,
        }
    }

    pub async fn refresh_all(&mut self) {
        self.error = None;
        self.fetch_ticker().await;
        self.fetch_prices().await;
        if let Some(ref c) = self.client {
            if let Ok(u) = c.request::<User, ()>(Method::GET, "account", None).await {
                self.user = Some(u);
            }
            // Fetch isolated positions
            let mut positions = Vec::new();
            if let Ok(t) = c
                .request::<Vec<Trade>, ()>(
                    Method::GET,
                    "futures/isolated/trades/running?limit=50",
                    None,
                )
                .await
            {
                positions.extend(t);
            }
            // Fetch cross margin position
            if let Ok(cross) = c
                .request::<serde_json::Value, ()>(Method::GET, "futures/cross/position", None)
                .await
            {
                let quantity = cross["quantity"].as_f64().unwrap_or(0.0);
                if quantity != 0.0 {
                    let side = if quantity > 0.0 { "buy" } else { "sell" };
                    let entry_price = cross["entryPrice"].as_f64();
                    let margin = cross["margin"].as_i64();
                    let leverage = cross["leverage"].as_f64().unwrap_or(1.0);
                    let pl = cross["pl"].as_i64();
                    let cross_trade = Trade {
                        id: "cross".to_string(),
                        user_id: None,
                        side: side.to_string(),
                        order_type: "market".to_string(),
                        quantity: quantity.abs() as i64,
                        leverage,
                        stop_loss: cross["stoploss"].as_f64(),
                        take_profit: cross["takeprofit"].as_f64(),
                        price: entry_price,
                        entry_price,
                        exit_price: None,
                        margin,
                        margin_with_cf: None,
                        pl,
                        liquidation_price: cross["liquidationPrice"].as_f64(),
                        created_at: None,
                        open_at: None,
                        closed_at: None,
                        last_update: None,
                        margin_type: MarginType::Cross,
                    };
                    positions.insert(0, cross_trade); // Cross position first
                }
            }
            self.positions = positions;
            if let Ok(t) = c
                .request::<Vec<Trade>, ()>(
                    Method::GET,
                    "futures/isolated/trades/open?limit=50",
                    None,
                )
                .await
            {
                self.orders = t;
            }
            if let Ok(t) = c
                .request::<Vec<Trade>, ()>(
                    Method::GET,
                    "futures/isolated/trades/closed?limit=20",
                    None,
                )
                .await
            {
                self.closed_trades = t;
            }
            // Deposits & withdrawals history
            if let Ok(d) = c
                .request::<Vec<Deposit>, ()>(
                    Method::GET,
                    "account/deposit/lightning?limit=20",
                    None,
                )
                .await
            {
                self.deposits = d;
            }
            if let Ok(w) = c
                .request::<Vec<Withdrawal>, ()>(Method::GET, "account/withdraw?limit=20", None)
                .await
            {
                self.withdrawals = w;
            }
        }
        // Recap (fetch once when tab active)
        if self.active_tab == Tab::Recap && self.recap.is_none() {
            self.recap = Some(crate::recap::fetch_market_recap().await);
        }
        self.last_refresh = chrono::Local::now().format("%H:%M:%S").to_string();
    }
    async fn fetch_ticker(&mut self) {
        if let Some(ref c) = self.client {
            if let Ok(t) = c
                .public_request::<Ticker>(Method::GET, "futures/ticker")
                .await
            {
                self.ticker = Some(t);
                return;
            }
        }
        if let Ok(c) = LnmClient::new(crate::config::Network::Mainnet, None) {
            if let Ok(t) = c
                .public_request::<Ticker>(Method::GET, "futures/ticker")
                .await
            {
                self.ticker = Some(t);
            }
        }
    }
    async fn fetch_prices(&mut self) {
        if let Some(ref c) = self.client {
            if let Ok(d) = c
                .public_request::<Vec<serde_json::Value>>(Method::GET, "oracle/index?limit=60")
                .await
            {
                self.price_history = d
                    .iter()
                    .filter_map(|v| v.get("value").or(v.get("index")).and_then(|x| x.as_f64()))
                    .collect();
                return;
            }
        }
        if let Ok(c) = LnmClient::new(crate::config::Network::Mainnet, None) {
            if let Ok(d) = c
                .public_request::<Vec<serde_json::Value>>(Method::GET, "oracle/index?limit=60")
                .await
            {
                self.price_history = d
                    .iter()
                    .filter_map(|v| v.get("value").or(v.get("index")).and_then(|x| x.as_f64()))
                    .collect();
            }
        }
    }
}
