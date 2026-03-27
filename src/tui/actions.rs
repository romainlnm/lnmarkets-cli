use super::app::App;
use super::popup::{ConfirmAction, FormAction, Notification, Popup};
use reqwest::Method;

/// Convert price to JSON value - integer if whole number, float otherwise.
/// This matters for HMAC signature: `72000` vs `72000.0` produce different signatures.
fn price_to_json(p: f64) -> serde_json::Value {
    if p.fract() == 0.0 {
        serde_json::Value::Number((p as i64).into())
    } else {
        serde_json::json!(p)
    }
}

impl App {
    pub async fn execute_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::ClosePosition(id) => self.close_pos(&id).await,
            ConfirmAction::CloseAllPositions => self.close_all().await,
            ConfirmAction::CancelOrder(id) => self.cancel_ord(&id).await,
            ConfirmAction::CancelAllOrders => self.cancel_all_ord().await,
            ConfirmAction::Logout => self.logout().await,
        }
    }
    pub async fn execute_form(&mut self, action: FormAction, f: &[super::popup::InputField]) {
        match action {
            FormAction::OpenPosition => self.open_pos(f).await,
            FormAction::UpdateStopLoss(id) => match f.first().and_then(|x| x.as_f64()) {
                Some(p) => self.set_sl(&id, p).await,
                _ => self.n_err("Invalid price"),
            },
            FormAction::UpdateTakeProfit(id) => match f.first().and_then(|x| x.as_f64()) {
                Some(p) => self.set_tp(&id, p).await,
                _ => self.n_err("Invalid price"),
            },
            FormAction::AddMargin(id) => match f.first().and_then(|x| x.as_i64()) {
                Some(a) => self.add_mgn(&id, a).await,
                _ => self.n_err("Invalid"),
            },
            FormAction::CashIn(id) => match f.first().and_then(|x| x.as_i64()) {
                Some(a) => self.cashin(&id, a).await,
                _ => self.n_err("Invalid"),
            },
            FormAction::DepositLightning => match f.first().and_then(|x| x.as_i64()) {
                Some(a) => self.dep_ln(a).await,
                _ => self.n_err("Invalid"),
            },
            FormAction::WithdrawLightning => {
                let a = f.first().and_then(|x| x.as_i64());
                let i = f.get(1).map(|x| x.value.clone()).filter(|v| !v.is_empty());
                match (a, i) {
                    (Some(a), Some(i)) => self.wd_ln(a, &i).await,
                    _ => self.n_err("Amount+invoice"),
                }
            }
            FormAction::WithdrawOnchain => {
                let a = f.first().and_then(|x| x.as_i64());
                let ad = f.get(1).map(|x| x.value.clone()).filter(|v| !v.is_empty());
                match (a, ad) {
                    (Some(a), Some(ad)) => self.wd_oc(a, &ad).await,
                    _ => self.n_err("Amount+address"),
                }
            }
            FormAction::NewBitcoinAddress => self.new_addr().await,
            FormAction::Login => self.login(f).await,
            FormAction::LaunchDaemon => self.daemon_launch(f).await,
            FormAction::UpdateAccount => self.update_account(f).await,
        }
    }
    fn n_err(&mut self, m: &str) {
        self.notify(Notification::error(m));
    }

    // ── Open position: exact same as CLI cli/futures.rs ──
    // side: "buy"/"sell", type: "market"/"limit", leverage: integer
    // endpoint: "futures/isolated/trade" (POST)
    async fn open_pos(&mut self, f: &[super::popup::InputField]) {
        let c = match &self.client {
            Some(c) => c,
            _ => {
                self.n_err("Not auth");
                return;
            }
        };
        let ss = f.first().map(|x| x.value.clone()).unwrap_or_default();
        let side = match ss.to_lowercase().as_str() {
            "buy" | "b" | "long" | "l" => "buy",
            "sell" | "s" | "short" => "sell",
            _ => {
                self.n_err("Side: buy/sell");
                return;
            }
        };
        let q = match f.get(1).and_then(|x| x.as_i64()) {
            Some(q) if q > 0 => q,
            _ => {
                self.n_err("Invalid qty");
                return;
            }
        };
        let l = match f.get(2).and_then(|x| x.as_i64()) {
            Some(l) if (1..=100).contains(&l) => l,
            _ => {
                self.n_err("Lev 1-100");
                return;
            }
        };
        let p = f.get(3).and_then(|x| x.as_f64());
        let sl = f.get(4).and_then(|x| x.as_f64());
        let tp = f.get(5).and_then(|x| x.as_f64());
        let ot = if p.filter(|v| *v > 0.0).is_some() {
            "limit"
        } else {
            "market"
        };
        let mut r = serde_json::json!({"side":side,"type":ot,"quantity":q,"leverage":l});
        if let Some(v) = p {
            if v > 0.0 {
                r["price"] = serde_json::json!(v);
            }
        }
        if let Some(v) = sl {
            if v > 0.0 {
                r["stoploss"] = serde_json::json!(v);
            }
        }
        if let Some(v) = tp {
            if v > 0.0 {
                r["takeprofit"] = serde_json::json!(v);
            }
        }
        match c
            .request::<serde_json::Value, _>(Method::POST, "futures/isolated/trade", Some(&r))
            .await
        {
            Ok(r) if r.get("id").is_some() => {
                self.notify(Notification::success(format!(
                    "Opened {} {} @{}x",
                    if side == "buy" { "LONG" } else { "SHORT" },
                    q,
                    l
                )));
                self.refresh_all().await;
            }
            Ok(r) => self.notify(Notification::error(format!("{}", r))),
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // endpoint: "futures/isolated/trade/close" (POST) {id}
    async fn close_pos(&mut self, id: &str) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::POST,
                "futures/isolated/trade/close",
                Some(&serde_json::json!({"id":id})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!(
                    "Closed {}",
                    &id[..8.min(id.len())]
                )));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    async fn close_all(&mut self) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        let ids: Vec<String> = self.positions.iter().map(|p| p.id.clone()).collect();
        let mut ok = 0;
        for id in &ids {
            if c.request::<serde_json::Value, _>(
                Method::POST,
                "futures/isolated/trade/close",
                Some(&serde_json::json!({"id":id})),
            )
            .await
            .is_ok()
            {
                ok += 1;
            }
        }
        self.notify(Notification::success(format!(
            "Closed {}/{}",
            ok,
            ids.len()
        )));
        self.refresh_all().await;
    }
    // endpoint: "futures/isolated/trade/cancel" (POST) {id}
    async fn cancel_ord(&mut self, id: &str) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::POST,
                "futures/isolated/trade/cancel",
                Some(&serde_json::json!({"id":id})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!(
                    "Cancelled {}",
                    &id[..8.min(id.len())]
                )));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // endpoint: "futures/isolated/trades/cancel-all" (POST)
    async fn cancel_all_ord(&mut self) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, ()>(
                Method::POST,
                "futures/isolated/trades/cancel-all",
                None,
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success("All cancelled"));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // endpoint: "futures/isolated/trade/stoploss" (PUT) {id,value}
    async fn set_sl(&mut self, id: &str, p: f64) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::PUT,
                "futures/isolated/trade/stoploss",
                Some(&serde_json::json!({"id": id, "value": price_to_json(p)})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!("SL→${:.0}", p)));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // endpoint: "futures/isolated/trade/takeprofit" (PUT) {id,value}
    async fn set_tp(&mut self, id: &str, p: f64) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::PUT,
                "futures/isolated/trade/takeprofit",
                Some(&serde_json::json!({"id": id, "value": price_to_json(p)})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!("TP→${:.0}", p)));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // endpoint: "futures/isolated/trade/add-margin" (POST) {id,amount}
    async fn add_mgn(&mut self, id: &str, a: i64) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::POST,
                "futures/isolated/trade/add-margin",
                Some(&serde_json::json!({"id":id,"amount":a})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!("+{} margin", a)));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // endpoint: "futures/isolated/trade/cash-in" (POST) {id,amount}
    async fn cashin(&mut self, id: &str, a: i64) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::POST,
                "futures/isolated/trade/cash-in",
                Some(&serde_json::json!({"id":id,"amount":a})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!("Cashed {}", a)));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }

    // ── Funding: endpoints from cli/funding.rs ──
    // deposit: "account/deposit/lightning" (POST) {amount} → LightningInvoice.payment_request
    async fn dep_ln(&mut self, amount: i64) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::POST,
                "account/deposit/lightning",
                Some(&serde_json::json!({"amount":amount})),
            )
            .await
        {
            Ok(r) => {
                // LightningInvoice struct uses payment_request (snake_case), but JSON might be camelCase
                let inv = r
                    .get("payment_request")
                    .or(r.get("paymentRequest"))
                    .and_then(|v| v.as_str());
                if let Some(inv) = inv {
                    let mut lines: Vec<(String, String)> = vec![("Amount".into(), format!("{} sats", amount))];
                    // QR code
                    let mut qr_lines = Vec::new();
                    if let Ok(qr) = qrcode::QrCode::with_error_correction_level(inv.to_uppercase().as_bytes(), qrcode::EcLevel::L) {
                        let w = qr.width();
                        let colors = qr.to_colors();
                        let border = "█".repeat(w + 4);
                        qr_lines.push(border.clone());
                        qr_lines.push(border.clone());
                        let mut y = 0;
                        while y < w {
                            let mut s = String::from("██");
                            for x in 0..w {
                                let t = colors[y * w + x] == qrcode::Color::Light;
                                let b = if y+1 < w { colors[(y+1) * w + x] == qrcode::Color::Light } else { true };
                                s.push(match (t,b) { (true,true)=>'█', (true,false)=>'▀', (false,true)=>'▄', (false,false)=>' ' });
                            }
                            s.push_str("██");
                            qr_lines.push(s);
                            y += 2;
                        }
                        qr_lines.push(border.clone());
                        qr_lines.push(border);
                    }
                    if !qr_lines.is_empty() {
                        self.popup = Some(Popup::QrCode { title: "⚡ Lightning Invoice".into(), invoice: inv.to_string(), amount, qr_lines });
                    } else {
                        let mut lines: Vec<(String, String)> = vec![("Amount".into(), format!("{} sats", amount)), ("".into(), String::new())];
                        let chars: Vec<char> = inv.chars().collect();
                        for chunk in chars.chunks(45) { lines.push(("".into(), chunk.iter().collect())); }
                        lines.push(("".into(), String::new()));
                        lines.push(("".into(), "Copy invoice to pay".into()));
                        self.popup = Some(Popup::Detail { title: "⚡ Lightning Invoice".into(), lines });
                    }
                } else {
                    self.notify(Notification::info(format!("{}", r)));
                }
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // withdraw lightning: "account/withdraw/lightning" (POST) {amount,invoice}
    async fn wd_ln(&mut self, amount: i64, invoice: &str) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::POST,
                "account/withdraw/lightning",
                Some(&serde_json::json!({"amount":amount,"invoice":invoice})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!("Withdrew {} ⚡", amount)));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // withdraw onchain: "account/withdraw/on-chain" (POST) {amount,address}
    async fn wd_oc(&mut self, amount: i64, address: &str) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, _>(
                Method::POST,
                "account/withdraw/on-chain",
                Some(&serde_json::json!({"amount":amount,"address":address})),
            )
            .await
        {
            Ok(_) => {
                self.notify(Notification::success(format!(
                    "Withdrew {} on-chain",
                    amount
                )));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }
    // new address: "account/address/bitcoin" (POST)
    async fn new_addr(&mut self) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        match c
            .request::<serde_json::Value, ()>(Method::POST, "account/address/bitcoin", None)
            .await
        {
            Ok(r) => {
                if let Some(a) = r.get("address").and_then(|v| v.as_str()) {
                    self.popup = Some(Popup::Detail {
                        title: "₿ Address".into(),
                        lines: vec![
                            ("Address".into(), a.to_string()),
                            ("".into(), "Send BTC here".into()),
                        ],
                    });
                    self.notify(Notification::success("Address generated"));
                } else {
                    self.notify(Notification::info(format!("{}", r)));
                }
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }

    // ── Login: create client + save config ──
    async fn login(&mut self, fields: &[super::popup::InputField]) {
        let k = fields
            .first()
            .map(|f| f.value.clone())
            .filter(|v| !v.is_empty());
        let s = fields
            .get(1)
            .map(|f| f.value.clone())
            .filter(|v| !v.is_empty());
        let p = fields
            .get(2)
            .map(|f| f.value.clone())
            .filter(|v| !v.is_empty());
        match (k, s, p) {
            (Some(k), Some(s), Some(p)) => {
                let creds = crate::config::Credentials {
                    api_key: Some(k),
                    api_secret: Some(s),
                    passphrase: Some(p),
                };
                // Save to config
                let mut cfg = crate::config::Config::load().unwrap_or_default();
                cfg.credentials = creds.clone();
                if let Err(e) = cfg.save() {
                    self.n_err(&format!("Save: {}", e));
                    return;
                }
                let net = if self.use_testnet {
                    crate::config::Network::Testnet
                } else {
                    crate::config::Network::Mainnet
                };
                match crate::api::LnmClient::new(net, Some(creds)) {
                    Ok(c) => {
                        self.client = Some(c);
                        self.authenticated = true;
                        self.notify(Notification::success("Logged in & saved"));
                        self.refresh_all().await;
                    }
                    Err(e) => self.n_err(&format!("{}", e)),
                }
            }
            _ => self.n_err("All 3 fields required"),
        }
    }

    // ── Logout ──
    async fn logout(&mut self) {
        let mut cfg = crate::config::Config::load().unwrap_or_default();
        cfg.credentials = crate::config::Credentials::default();
        let _ = cfg.save();
        self.client = None;
        self.authenticated = false;
        self.user = None;
        self.positions.clear();
        self.orders.clear();
        self.notify(Notification::success("Logged out"));
    }

    // ── Update account: "account" (PUT) ──
    async fn update_account(&mut self, fields: &[super::popup::InputField]) {
        let c = match &self.client {
            Some(c) => c,
            _ => return,
        };
        let un = fields
            .first()
            .map(|f| f.value.clone())
            .filter(|v| !v.is_empty());
        let sl = fields
            .get(1)
            .map(|f| f.value.to_lowercase())
            .map(|v| v.starts_with('y'));
        let mut req = serde_json::json!({});
        if let Some(u) = un {
            req["username"] = serde_json::json!(u);
        }
        if let Some(s) = sl {
            req["showLeaderboard"] = serde_json::json!(s);
        }
        match c
            .request::<serde_json::Value, _>(Method::PUT, "account", Some(&req))
            .await
        {
            Ok(_) => {
                self.notify(Notification::success("Account updated"));
                self.refresh_all().await;
            }
            Err(e) => self.notify(Notification::error(format!("{}", e))),
        }
    }

    // ── Daemon launch ──
    async fn daemon_launch(&mut self, fields: &[super::popup::InputField]) {
        let agents_s = fields
            .first()
            .map(|f| f.value.clone())
            .unwrap_or("pattern".into());
        let interval = fields.get(1).and_then(|f| f.as_u64()).unwrap_or(60);
        let mode_s = fields
            .get(2)
            .map(|f| f.value.clone())
            .unwrap_or("dry".into());
        let min_c = fields.get(3).and_then(|f| f.as_f64()).unwrap_or(0.7);
        let max_p = fields.get(4).and_then(|f| f.as_u64()).unwrap_or(10);
        let lev = fields.get(5).and_then(|f| f.as_u32()).unwrap_or(10);
        let mode = match mode_s.to_lowercase().as_str() {
            "live" => crate::daemon::TradingMode::Live,
            "paper" => crate::daemon::TradingMode::Paper,
            _ => crate::daemon::TradingMode::DryRun,
        };
        let agents: Vec<String> = agents_s.split(',').map(|s| s.trim().to_string()).collect();
        let dcfg = crate::daemon::DaemonConfig {
            interval_secs: interval,
            mode,
            min_confidence: min_c,
            max_position_usd: max_p,
            leverage: lev,
            take_profit_pct: Some(5.0),
            stop_loss_pct: Some(3.0),
            trailing_stop_pct: Some(3.0), // 3% trailing stop by default
            agents: agents.clone(),
            reversal_cooldown_secs: 300, // 5 minute cooldown
            conflict_threshold: 0.3,     // Skip if agents disagree by <30%
            min_atr_pct: None,           // ATR filter disabled in TUI for now
            treasury: None,              // TUI doesn't support treasury yet
        };
        let client = if mode == crate::daemon::TradingMode::Live {
            let cfg = crate::config::Config::load().unwrap_or_default();
            let cr = cfg.get_credentials();
            let net = if self.use_testnet {
                crate::config::Network::Testnet
            } else {
                crate::config::Network::Mainnet
            };
            crate::api::LnmClient::new(net, Some(cr)).ok()
        } else {
            None
        };
        self.daemon_status = Some(format!("{} agents={} int={}s", mode_s, agents_s, interval));
        self.notify(Notification::success(format!(
            "Daemon started ({})",
            mode_s
        )));
        let d = crate::daemon::Daemon::new(dcfg, client);
        tokio::spawn(async move {
            let _ = d.run().await;
        });
    }
}
