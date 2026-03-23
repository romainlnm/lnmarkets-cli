use std::time::Instant;

#[derive(Clone)]
pub struct Notification {
    pub message: String,
    pub kind: NotifKind,
    pub created: Instant,
    pub ttl_secs: u64,
}
#[derive(Clone, Copy, PartialEq)]
pub enum NotifKind {
    Success,
    Error,
    Info,
}
impl Notification {
    pub fn success(m: impl Into<String>) -> Self {
        Self {
            message: m.into(),
            kind: NotifKind::Success,
            created: Instant::now(),
            ttl_secs: 5,
        }
    }
    pub fn error(m: impl Into<String>) -> Self {
        Self {
            message: m.into(),
            kind: NotifKind::Error,
            created: Instant::now(),
            ttl_secs: 8,
        }
    }
    pub fn info(m: impl Into<String>) -> Self {
        Self {
            message: m.into(),
            kind: NotifKind::Info,
            created: Instant::now(),
            ttl_secs: 4,
        }
    }
    pub fn is_expired(&self) -> bool {
        self.created.elapsed().as_secs() >= self.ttl_secs
    }
}
#[derive(Clone)]
pub struct InputField {
    pub label: String,
    pub value: String,
    pub placeholder: String,
    pub field_type: FieldType,
}
#[derive(Clone, Copy, PartialEq)]
pub enum FieldType {
    Text,
    Number,
    Price,
}
impl InputField {
    pub fn new(l: &str, p: &str, ft: FieldType) -> Self {
        Self {
            label: l.into(),
            value: String::new(),
            placeholder: p.into(),
            field_type: ft,
        }
    }
    pub fn push(&mut self, c: char) {
        match self.field_type {
            FieldType::Text => self.value.push(c),
            FieldType::Number => {
                if c.is_ascii_digit() {
                    self.value.push(c);
                }
            }
            FieldType::Price => {
                if c.is_ascii_digit() || (c == '.' && !self.value.contains('.')) {
                    self.value.push(c);
                }
            }
        }
    }
    pub fn pop(&mut self) {
        self.value.pop();
    }
    pub fn as_f64(&self) -> Option<f64> {
        if self.value.is_empty() {
            None
        } else {
            self.value.parse().ok()
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        if self.value.is_empty() {
            None
        } else {
            self.value.parse().ok()
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        if self.value.is_empty() {
            None
        } else {
            self.value.parse().ok()
        }
    }
    pub fn as_u32(&self) -> Option<u32> {
        if self.value.is_empty() {
            None
        } else {
            self.value.parse().ok()
        }
    }
}
#[derive(Clone)]
pub enum Popup {
    Confirm {
        title: String,
        message: String,
        action: ConfirmAction,
    },
    Form {
        title: String,
        fields: Vec<InputField>,
        active_field: usize,
        action: FormAction,
    },
    Detail {
        title: String,
        lines: Vec<(String, String)>,
    },
    QrCode { title: String, invoice: String, amount: i64, qr_lines: Vec<String> },
    Help,
}
#[derive(Clone, Debug)]
pub enum ConfirmAction {
    ClosePosition(String),
    CloseAllPositions,
    CancelOrder(String),
    CancelAllOrders,
    Logout,
}
#[derive(Clone, Debug)]
pub enum FormAction {
    OpenPosition,
    UpdateStopLoss(String),
    UpdateTakeProfit(String),
    AddMargin(String),
    CashIn(String),
    DepositLightning,
    WithdrawLightning,
    WithdrawOnchain,
    NewBitcoinAddress,
    Login,
    LaunchDaemon,
    UpdateAccount,
}
impl Popup {
    pub fn open_position() -> Self {
        Popup::Form {
            title: "Open Position".into(),
            fields: vec![
                InputField::new("Side (buy/sell)", "buy", FieldType::Text),
                InputField::new("Quantity (USD)", "100", FieldType::Number),
                InputField::new("Leverage (1-100)", "10", FieldType::Number),
                InputField::new("Price (0=market)", "0", FieldType::Price),
                InputField::new("Stop Loss (0=none)", "0", FieldType::Price),
                InputField::new("Take Profit (0=none)", "0", FieldType::Price),
            ],
            active_field: 0,
            action: FormAction::OpenPosition,
        }
    }
    pub fn update_stoploss(id: &str, c: Option<f64>) -> Self {
        let p = c
            .filter(|v| *v > 0.0)
            .map(|v| format!("{:.0}", v))
            .unwrap_or("price".into());
        Popup::Form {
            title: format!("SL — {}", &id[..8.min(id.len())]),
            fields: vec![InputField::new("Price", &p, FieldType::Price)],
            active_field: 0,
            action: FormAction::UpdateStopLoss(id.into()),
        }
    }
    pub fn update_takeprofit(id: &str, c: Option<f64>) -> Self {
        let p = c
            .filter(|v| *v > 0.0)
            .map(|v| format!("{:.0}", v))
            .unwrap_or("price".into());
        Popup::Form {
            title: format!("TP — {}", &id[..8.min(id.len())]),
            fields: vec![InputField::new("Price", &p, FieldType::Price)],
            active_field: 0,
            action: FormAction::UpdateTakeProfit(id.into()),
        }
    }
    pub fn add_margin(id: &str) -> Self {
        Popup::Form {
            title: format!("Margin — {}", &id[..8.min(id.len())]),
            fields: vec![InputField::new("Sats", "1000", FieldType::Number)],
            active_field: 0,
            action: FormAction::AddMargin(id.into()),
        }
    }
    pub fn cash_in(id: &str) -> Self {
        Popup::Form {
            title: format!("Cash In — {}", &id[..8.min(id.len())]),
            fields: vec![InputField::new("Sats", "500", FieldType::Number)],
            active_field: 0,
            action: FormAction::CashIn(id.into()),
        }
    }
    pub fn confirm_close(id: &str, side: &str, qty: i64, pl: i64) -> Self {
        Popup::Confirm {
            title: "Close".into(),
            message: format!(
                "Close {} {} USD?\nP&L: {}{} sats\n\nY / N",
                side.to_uppercase(),
                qty,
                if pl >= 0 { "+" } else { "" },
                pl
            ),
            action: ConfirmAction::ClosePosition(id.into()),
        }
    }
    pub fn confirm_close_all(n: usize) -> Self {
        Popup::Confirm {
            title: "Close ALL".into(),
            message: format!("Close {} positions?\nY / N", n),
            action: ConfirmAction::CloseAllPositions,
        }
    }
    pub fn confirm_cancel(id: &str) -> Self {
        Popup::Confirm {
            title: "Cancel".into(),
            message: format!("Cancel {}?\nY / N", &id[..8.min(id.len())]),
            action: ConfirmAction::CancelOrder(id.into()),
        }
    }
    pub fn confirm_cancel_all(n: usize) -> Self {
        Popup::Confirm {
            title: "Cancel ALL".into(),
            message: format!("Cancel {} orders?\nY / N", n),
            action: ConfirmAction::CancelAllOrders,
        }
    }
    pub fn confirm_logout() -> Self {
        Popup::Confirm {
            title: "Logout".into(),
            message: "Remove credentials?\nY / N".into(),
            action: ConfirmAction::Logout,
        }
    }
    pub fn deposit_lightning() -> Self {
        Popup::Form {
            title: "Deposit ⚡".into(),
            fields: vec![InputField::new("Amount (sats)", "10000", FieldType::Number)],
            active_field: 0,
            action: FormAction::DepositLightning,
        }
    }
    pub fn withdraw_lightning() -> Self {
        Popup::Form {
            title: "Withdraw ⚡".into(),
            fields: vec![
                InputField::new("Amount (sats)", "5000", FieldType::Number),
                InputField::new("Invoice", "lnbc...", FieldType::Text),
            ],
            active_field: 0,
            action: FormAction::WithdrawLightning,
        }
    }
    pub fn withdraw_onchain() -> Self {
        Popup::Form {
            title: "Withdraw ₿".into(),
            fields: vec![
                InputField::new("Amount (sats)", "100000", FieldType::Number),
                InputField::new("BTC Address", "bc1q...", FieldType::Text),
            ],
            active_field: 0,
            action: FormAction::WithdrawOnchain,
        }
    }
    pub fn new_btc_address() -> Self {
        Popup::Form {
            title: "New BTC Address".into(),
            fields: vec![InputField::new("Enter to generate", "", FieldType::Text)],
            active_field: 0,
            action: FormAction::NewBitcoinAddress,
        }
    }
    pub fn login() -> Self {
        Popup::Form {
            title: "⚡ Login".into(),
            fields: vec![
                InputField::new("API Key", "", FieldType::Text),
                InputField::new("API Secret", "", FieldType::Text),
                InputField::new("Passphrase", "", FieldType::Text),
            ],
            active_field: 0,
            action: FormAction::Login,
        }
    }
    pub fn launch_daemon() -> Self {
        Popup::Form {
            title: "Daemon".into(),
            fields: vec![
                InputField::new("Agents", "pattern,flow", FieldType::Text),
                InputField::new("Interval (s)", "60", FieldType::Number),
                InputField::new("Mode (dry/paper/live)", "dry", FieldType::Text),
                InputField::new("Min Confidence", "0.7", FieldType::Price),
                InputField::new("Max Position USD", "10", FieldType::Number),
                InputField::new("Leverage", "10", FieldType::Number),
            ],
            active_field: 0,
            action: FormAction::LaunchDaemon,
        }
    }
    pub fn update_account() -> Self {
        Popup::Form {
            title: "Update Account".into(),
            fields: vec![
                InputField::new("Username", "", FieldType::Text),
                InputField::new("Show Leaderboard (y/n)", "y", FieldType::Text),
            ],
            active_field: 0,
            action: FormAction::UpdateAccount,
        }
    }
    pub fn position_detail(
        id: &str,
        side: &str,
        qty: i64,
        lev: f64,
        entry: Option<f64>,
        margin: Option<i64>,
        pl: Option<i64>,
        sl: Option<f64>,
        tp: Option<f64>,
    ) -> Self {
        let mut l = vec![
            ("ID".into(), id.into()),
            ("Side".into(), side.to_uppercase()),
            ("Qty".into(), format!("{} USD", qty)),
            ("Lev".into(), format!("{}x", lev)),
        ];
        if let Some(e) = entry {
            l.push(("Entry".into(), format!("${:.0}", e)));
        }
        if let Some(m) = margin {
            l.push(("Margin".into(), format!("{} sats", m)));
        }
        if let Some(p) = pl {
            l.push((
                "P&L".into(),
                format!("{}{} sats", if p >= 0 { "+" } else { "" }, p),
            ));
        }
        if let Some(s) = sl {
            if s > 0.0 {
                l.push(("SL".into(), format!("${:.0}", s)));
            }
        }
        if let Some(t) = tp {
            if t > 0.0 {
                l.push(("TP".into(), format!("${:.0}", t)));
            }
        }
        Popup::Detail {
            title: format!("Position {}", &id[..8.min(id.len())]),
            lines: l,
        }
    }
    pub fn help() -> Self {
        Popup::Help
    }
}
