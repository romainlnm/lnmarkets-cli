use super::app::{App, Tab};
use super::popup::{FieldType, NotifKind, Popup};
use ratatui::{prelude::*, widgets::*};

fn or(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(255, 153, 0)
    } else {
        Color::Rgb(200, 120, 0)
    }
}
fn pu(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(130, 80, 223)
    } else {
        Color::Rgb(100, 60, 180)
    }
}
fn gr(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(80, 200, 120)
    } else {
        Color::Rgb(40, 160, 80)
    }
}
fn rd(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(220, 60, 60)
    } else {
        Color::Rgb(200, 40, 40)
    }
}
fn yl(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(230, 200, 60)
    } else {
        Color::Rgb(180, 150, 20)
    }
}
fn dm(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(100, 100, 110)
    } else {
        Color::Rgb(140, 140, 150)
    }
}
fn fg(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(220, 220, 230)
    } else {
        Color::Rgb(30, 30, 40)
    }
}
fn bg(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(16, 16, 24)
    } else {
        Color::Rgb(245, 245, 250)
    }
}
fn b2(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(22, 22, 34)
    } else {
        Color::Rgb(255, 255, 255)
    }
}
fn b3(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(30, 30, 48)
    } else {
        Color::Rgb(240, 240, 248)
    }
}
fn b4(a: &App) -> Color {
    if a.dark_theme {
        Color::Rgb(36, 36, 54)
    } else {
        Color::Rgb(230, 230, 240)
    }
}

pub fn draw(f: &mut Frame, a: &App) {
    let ar = f.area();
    f.render_widget(Block::default().style(Style::default().bg(bg(a))), ar);
    let ch = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(ar);
    hdr(f, a, ch[0]);
    body(f, a, ch[1]);
    ftr(f, a, ch[2]);
    notifs(f, a, ar);
    if let Some(ref p) = a.popup {
        pdraw(f, a, p, ar);
    }
}

fn hdr(f: &mut Frame, a: &App, ar: Rect) {
    let ch = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(ar);
    let ts: Vec<Line> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let s = if *t == a.active_tab {
                Style::default().fg(or(a)).bold()
            } else {
                Style::default().fg(dm(a))
            };
            Line::from(Span::styled(format!(" {}{} ", i + 1, t.label()), s))
        })
        .collect();
    let net = if a.use_testnet { " [TEST] " } else { "" };
    f.render_widget(
        Tabs::new(ts)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(dm(a)))
                    .title(Span::styled(
                        format!(" ⚡ LNM {}", net),
                        Style::default().fg(or(a)).bold(),
                    )),
            )
            .select(a.active_tab.index())
            .style(Style::default().bg(bg(a))),
        ch[0],
    );
    let txt = if let Some(ref t) = a.ticker {
        let b = t.prices.first().map(|p| p.bid_price).unwrap_or(0.0);
        let ak = t.prices.first().map(|p| p.ask_price).unwrap_or(0.0);
        format!(
            "BTC ${} │ {:.0}/{:.0} │ FR {}",
            fp(t.index),
            b,
            ak,
            t.funding_rate
                .map(|r| format!("{:.4}%", r * 100.0))
                .unwrap_or_default()
        )
    } else {
        "Loading...".into()
    };
    f.render_widget(
        Paragraph::new(txt)
            .style(Style::default().fg(fg(a)).bg(bg(a)))
            .alignment(Alignment::Right)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(dm(a))),
            ),
        ch[1],
    );
}

fn body(f: &mut Frame, a: &App, ar: Rect) {
    match a.active_tab {
        Tab::Dashboard => dash(f, a, ar),
        Tab::Positions => ttbl(f, a, ar, "Positions", &a.positions),
        Tab::Orders => ttbl(f, a, ar, "Orders", &a.orders),
        Tab::History => ttbl(f, a, ar, "History", &a.closed_trades),
        Tab::Funding => fund(f, a, ar),
        Tab::Recap => recap(f, a, ar),
    }
}

fn dash(f: &mut Frame, a: &App, ar: Rect) {
    let ch = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(6),
            Constraint::Length(5),
        ])
        .split(ar);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(ch[0]);
    let bl = if let Some(b) = a.balance() {
        vec![Line::from(vec![
            Span::styled("Balance    ", Style::default().fg(dm(a))),
            Span::styled(format!("{} sats", fs(b)), Style::default().fg(gr(a)).bold()),
        ])]
    } else if !a.authenticated {
        vec![Line::from(Span::styled(
            "L to login",
            Style::default().fg(dm(a)),
        ))]
    } else {
        vec![Line::from("...")]
    };
    f.render_widget(
        Paragraph::new(bl)
            .block(pb(a, " Balance "))
            .style(Style::default().bg(b2(a))),
        top[0],
    );
    let tpl: i64 = a.positions.iter().filter_map(|p| p.pl).sum();
    let pc = if tpl >= 0 { gr(a) } else { rd(a) };
    let mut ov = vec![
        Line::from(vec![
            Span::styled("Positions  ", Style::default().fg(dm(a))),
            Span::styled(
                format!("{}", a.positions.len()),
                Style::default().fg(fg(a)).bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("P&L        ", Style::default().fg(dm(a))),
            Span::styled(
                format!("{}{} sats", if tpl >= 0 { "+" } else { "" }, tpl),
                Style::default().fg(pc).bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Orders     ", Style::default().fg(dm(a))),
            Span::styled(format!("{}", a.orders.len()), Style::default().fg(fg(a))),
        ]),
    ];
    if let Some(ref ds) = a.daemon_status {
        ov.push(Line::from(vec![
            Span::styled("Daemon     ", Style::default().fg(dm(a))),
            Span::styled(ds.as_str(), Style::default().fg(pu(a))),
        ]));
    }
    f.render_widget(
        Paragraph::new(ov)
            .block(pb(a, " Overview "))
            .style(Style::default().bg(b2(a))),
        top[1],
    );
    // Sparkline
    if !a.price_history.is_empty() {
        let d: Vec<u64> = a.price_history.iter().map(|p| *p as u64).collect();
        let (mn, mx) = (
            d.iter().copied().min().unwrap_or(0),
            d.iter().copied().max().unwrap_or(1),
        );
        let n: Vec<u64> = d
            .iter()
            .map(|v| v.saturating_sub(mn.saturating_sub(100)))
            .collect();
        f.render_widget(
            Sparkline::default()
                .block(pb(a, " BTC ").title_bottom(Line::from(vec![
                    Span::styled(format!(" L${} ", fp(mn as f64)), Style::default().fg(rd(a))),
                    Span::styled(format!(" H${} ", fp(mx as f64)), Style::default().fg(gr(a))),
                ])))
                .data(&n)
                .style(Style::default().fg(or(a)).bg(b2(a))),
            ch[1],
        );
    } else {
        f.render_widget(
            Paragraph::new("...")
                .block(pb(a, " BTC "))
                .style(Style::default().fg(dm(a)).bg(b2(a)))
                .alignment(Alignment::Center),
            ch[1],
        );
    }
    // Pos preview
    if a.positions.is_empty() {
        f.render_widget(
            Paragraph::new("o open │ ? help │ i info │ u update")
                .block(pb(a, " Positions "))
                .style(Style::default().fg(dm(a)).bg(b2(a)))
                .alignment(Alignment::Center),
            ch[2],
        );
    } else {
        let rows: Vec<Row> = a.positions.iter().take(4).map(|t| trow(a, t)).collect();
        f.render_widget(
            Table::new(rows, tw())
                .header(
                    Row::new(["Side", "Qty", "Lev", "Entry", "P&L", "SL", "TP", "ID"])
                        .style(Style::default().fg(dm(a)).bold()),
                )
                .block(pb(a, " Positions "))
                .style(Style::default().bg(b2(a))),
            ch[2],
        );
    }
}

fn ttbl(f: &mut Frame, a: &App, ar: Rect, title: &str, trades: &[crate::models::futures::Trade]) {
    if trades.is_empty() {
        f.render_widget(
            Paragraph::new(if a.authenticated {
                "Empty"
            } else {
                "L to login"
            })
            .block(pb(a, &format!(" {} ", title)))
            .style(Style::default().fg(dm(a)).bg(b2(a)))
            .alignment(Alignment::Center),
            ar,
        );
        return;
    }
    let rows: Vec<Row> = trades.iter().map(|t| trow(a, t)).collect();
    let t_str = format!(" {} ({}) ", title, trades.len());
    let tbl = Table::new(rows, tw())
        .header(
            Row::new(["Side", "Qty", "Lev", "Entry", "P&L", "SL", "TP", "ID"])
                .style(Style::default().fg(or(a)).bold()),
        )
        .block(pb(a, &t_str))
        .style(Style::default().bg(b2(a)))
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 60)))
        .highlight_symbol("▸ ");
    let mut st = TableState::default();
    st.select(Some(a.selected_row));
    f.render_stateful_widget(tbl, ar, &mut st);
}

fn fund(f: &mut Frame, a: &App, ar: Rect) {
    let ch = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(ar);
    // Actions
    let mut l = vec![Line::from("")];
    if let Some(b) = a.balance() {
        l.push(Line::from(vec![
            Span::styled("  Balance       ", Style::default().fg(dm(a))),
            Span::styled(format!("{} sats", fs(b)), Style::default().fg(gr(a)).bold()),
        ]));
    }
    l.push(Line::from(""));
    l.push(Line::from(Span::styled(
        "  Actions:",
        Style::default().fg(or(a)).bold(),
    )));
    for (k, d) in [
        ("d", "Deposit ⚡"),
        ("w", "Withdraw ⚡"),
        ("W", "Withdraw On-Chain ₿"),
        ("a", "New BTC Address"),
    ] {
        l.push(Line::from(vec![
            Span::styled(format!("    {}", k), Style::default().fg(yl(a)).bold()),
            Span::styled(format!("  {}", d), Style::default().fg(fg(a))),
        ]));
    }
    f.render_widget(
        Paragraph::new(l)
            .block(pb(a, " Funding "))
            .style(Style::default().bg(b2(a))),
        ch[0],
    );
    // Deposits
    let drows: Vec<Row> = a
        .deposits
        .iter()
        .map(|d| {
            Row::new(vec![
                Cell::from(Span::styled(
                    d.amount
                        .map(|a| format!("{} sats", fs(a)))
                        .unwrap_or("—".into()),
                    Style::default().fg(gr(a)),
                )),
                Cell::from(d.status.as_deref().unwrap_or("—").to_string()),
                Cell::from(d.deposit_type.as_deref().unwrap_or("—").to_string()),
                Cell::from(Span::styled(
                    if d.id.len() > 8 {
                        d.id[..8].to_string()
                    } else {
                        d.id.clone()
                    },
                    Style::default().fg(dm(a)),
                )),
            ])
        })
        .collect();
    f.render_widget(
        Table::new(
            drows,
            [
                Constraint::Length(16),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Min(8),
            ],
        )
        .header(
            Row::new(["Amount", "Status", "Type", "ID"]).style(Style::default().fg(gr(a)).bold()),
        )
        .block(pb(a, " Deposits "))
        .style(Style::default().bg(b2(a))),
        ch[1],
    );
    // Withdrawals
    let wrows: Vec<Row> = a
        .withdrawals
        .iter()
        .map(|w| {
            Row::new(vec![
                Cell::from(Span::styled(
                    w.amount
                        .map(|a| format!("{} sats", fs(a)))
                        .unwrap_or("—".into()),
                    Style::default().fg(rd(a)),
                )),
                Cell::from(w.status.as_deref().unwrap_or("—").to_string()),
                Cell::from(w.withdrawal_type.as_deref().unwrap_or("—").to_string()),
                Cell::from(Span::styled(
                    if w.id.len() > 8 {
                        w.id[..8].to_string()
                    } else {
                        w.id.clone()
                    },
                    Style::default().fg(dm(a)),
                )),
            ])
        })
        .collect();
    f.render_widget(
        Table::new(
            wrows,
            [
                Constraint::Length(16),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Min(8),
            ],
        )
        .header(
            Row::new(["Amount", "Status", "Type", "ID"]).style(Style::default().fg(rd(a)).bold()),
        )
        .block(pb(a, " Withdrawals "))
        .style(Style::default().bg(b2(a))),
        ch[2],
    );
}

fn recap(f: &mut Frame, a: &App, ar: Rect) {
    let ch = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(8), Constraint::Length(8)])
        .split(ar);
    let mut l = vec![
        Line::from(Span::styled(
            "BTC Market Recap",
            Style::default().fg(or(a)).bold(),
        )),
        Line::from(""),
    ];
    if let Some(ref r) = a.recap {
        if let Some(ref p) = r.price {
            l.push(kv(
                a,
                "  Price         ",
                &format!("${}", fp(p.current)),
                fg(a),
            ));
            l.push(kv(
                a,
                "  24h High      ",
                &format!("${} ({:+.1}%)", fp(p.high_24h), p.high_pct),
                gr(a),
            ));
            l.push(kv(
                a,
                "  24h Low       ",
                &format!("${} ({:+.1}%)", fp(p.low_24h), p.low_pct),
                rd(a),
            ));
            l.push(kv(
                a,
                "  24h Change    ",
                &format!("{:+.1}%", p.change_24h_pct),
                if p.change_24h_pct >= 0.0 {
                    gr(a)
                } else {
                    rd(a)
                },
            ));
        }
        if let Some(ref d) = r.derivatives {
            l.push(Line::from(""));
            l.push(kv(
                a,
                "  Funding       ",
                &format!("{:.4}% ({})", d.funding_rate, d.funding_sentiment.label()),
                if d.funding_rate >= 0.0 { gr(a) } else { rd(a) },
            ));
            l.push(kv(
                a,
                "  Open Interest ",
                &format!("${:.1}B", d.open_interest / 1e9),
                fg(a),
            ));
            l.push(kv(
                a,
                "  L/S Ratio     ",
                &format!("{:.2} ({})", d.long_short_ratio, d.ls_sentiment.label()),
                fg(a),
            ));
        }
        if let Some(ref s) = r.sentiment {
            l.push(Line::from(""));
            l.push(kv(
                a,
                "  Fear & Greed  ",
                &format!("{} ({}) {}", s.value, s.label, s.change_indicator()),
                if s.value >= 60 {
                    gr(a)
                } else if s.value <= 40 {
                    rd(a)
                } else {
                    yl(a)
                },
            ));
        }
        for e in &r.errors {
            l.push(Line::from(Span::styled(
                format!("  ⚠ {}", e),
                Style::default().fg(rd(a)),
            )));
        }
    } else if let Some(ref t) = a.ticker {
        let b = t.prices.first().map(|p| p.bid_price).unwrap_or(0.0);
        l.push(kv(
            a,
            "  Index         ",
            &format!("${}", fp(t.index)),
            fg(a),
        ));
        l.push(kv(
            a,
            "  Bid/Ask       ",
            &format!(
                "{:.0}/{:.0}",
                b,
                t.prices.first().map(|p| p.ask_price).unwrap_or(0.0)
            ),
            fg(a),
        ));
        l.push(Line::from(Span::styled(
            "  Loading recap...",
            Style::default().fg(dm(a)),
        )));
    } else {
        l.push(Line::from(Span::styled(
            "  Loading...",
            Style::default().fg(dm(a)),
        )));
    }
    f.render_widget(
        Paragraph::new(l)
            .block(pb(a, " Recap "))
            .style(Style::default().bg(b2(a))),
        ch[0],
    );
    // Calendar
    if let Some(ref r) = a.recap {
        let mut el = vec![];
        if !r.recent_events.is_empty() {
            el.push(Line::from(Span::styled(
                "Recent",
                Style::default().fg(pu(a)).bold(),
            )));
            for e in r.recent_events.iter().take(3) {
                let s = e
                    .surprise_pct
                    .map(|s| format!(" ({:+.1}%)", s))
                    .unwrap_or_default();
                let im = e
                    .btc_impact
                    .as_ref()
                    .map(|i| format!(" → {}", i.label()))
                    .unwrap_or_default();
                el.push(Line::from(Span::styled(
                    format!("  {} {}{}{}", e.importance.icon(), e.title, s, im),
                    Style::default().fg(fg(a)),
                )));
            }
        }
        if !r.upcoming_events.is_empty() {
            el.push(Line::from(Span::styled(
                "Upcoming",
                Style::default().fg(pu(a)).bold(),
            )));
            for e in r.upcoming_events.iter().take(3) {
                let m = e.minutes_until;
                let w = if m < 60 {
                    format!("{}m", m)
                } else if m < 1440 {
                    format!("{}h", m / 60)
                } else {
                    format!("{}d", m / 1440)
                };
                el.push(Line::from(Span::styled(
                    format!("  {} {} in {}", e.importance.icon(), e.title, w),
                    Style::default().fg(fg(a)),
                )));
            }
        }
        if el.is_empty() {
            el.push(Line::from(Span::styled(
                "No events",
                Style::default().fg(dm(a)),
            )));
        }
        f.render_widget(
            Paragraph::new(el)
                .block(pb(a, " Calendar "))
                .style(Style::default().bg(b2(a))),
            ch[1],
        );
    } else {
        f.render_widget(
            Paragraph::new("...")
                .block(pb(a, " Calendar "))
                .style(Style::default().fg(dm(a)).bg(b2(a)))
                .alignment(Alignment::Center),
            ch[1],
        );
    }
}

fn ftr(f: &mut Frame, a: &App, ar: Rect) {
    let ch = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(ar);
    f.render_widget(
        Paragraph::new(Line::from(ckeys(a))).style(Style::default().bg(bg(a))),
        ch[0],
    );
    let mut sp = vec![
        Span::styled(" q", Style::default().fg(or(a)).bold()),
        Span::styled(" quit ", Style::default().fg(dm(a))),
        Span::styled("?", Style::default().fg(or(a)).bold()),
        Span::styled(" help ", Style::default().fg(dm(a))),
        Span::styled("o", Style::default().fg(or(a)).bold()),
        Span::styled(" open ", Style::default().fg(dm(a))),
        Span::styled("T", Style::default().fg(or(a)).bold()),
        Span::styled(" theme ", Style::default().fg(dm(a))),
        Span::styled("N", Style::default().fg(or(a)).bold()),
        Span::styled(" net ", Style::default().fg(dm(a))),
    ];
    if a.authenticated {
        sp.push(Span::styled("D", Style::default().fg(or(a)).bold()));
        sp.push(Span::styled(" daemon ", Style::default().fg(dm(a))));
        sp.push(Span::styled("O", Style::default().fg(or(a)).bold()));
        sp.push(Span::styled(" logout ", Style::default().fg(dm(a))));
    } else {
        sp.push(Span::styled("L", Style::default().fg(yl(a)).bold()));
        sp.push(Span::styled(" login ", Style::default().fg(dm(a))));
    }
    if !a.last_refresh.is_empty() {
        sp.push(Span::styled(
            format!(" │ ⟳ {}({}s)", a.last_refresh, a.refresh_secs),
            Style::default().fg(dm(a)),
        ));
    }
    if let Some(ref e) = a.error {
        sp.push(Span::styled(
            format!(" │ ⚠ {}", e),
            Style::default().fg(rd(a)),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(sp)).style(Style::default().bg(bg(a))),
        ch[1],
    );
}

fn ckeys(a: &App) -> Vec<Span<'static>> {
    match a.active_tab {
        Tab::Dashboard => vec![
            ks(a, "i", "info"),
            ks(a, "u", "update"),
            ks(a, "e", "export"),
        ]
        .into_iter()
        .flatten()
        .collect(),
        Tab::Positions => vec![
            ks(a, "⏎", "detail"),
            ks(a, "c", "close"),
            ks(a, "C", "all"),
            ks(a, "s", "SL"),
            ks(a, "t", "TP"),
            ks(a, "m", "margin"),
            ks(a, "$", "cash"),
        ]
        .into_iter()
        .flatten()
        .collect(),
        Tab::Orders => vec![ks(a, "x", "cancel"), ks(a, "X", "all")]
            .into_iter()
            .flatten()
            .collect(),
        Tab::Funding => vec![
            ks(a, "d", "dep⚡"),
            ks(a, "w", "wd⚡"),
            ks(a, "W", "chain"),
            ks(a, "a", "addr"),
        ]
        .into_iter()
        .flatten()
        .collect(),
        Tab::History => vec![ks(a, "⏎", "detail")]
            .into_iter()
            .flatten()
            .collect(),
        _ => vec![Span::styled(
            " ↑↓ select │ 1-6 tabs",
            Style::default().fg(dm(a)),
        )],
    }
}
fn ks(a: &App, k: &str, d: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!(" {}", k), Style::default().fg(yl(a)).bold()),
        Span::styled(format!(" {}", d), Style::default().fg(dm(a))),
    ]
}

fn notifs(f: &mut Frame, a: &App, ar: Rect) {
    for (i, n) in a.notifications.iter().rev().take(3).enumerate() {
        let w = (n.message.len() as u16 + 6).min(ar.width.saturating_sub(4));
        let (x, y) = (ar.width.saturating_sub(w + 2), 4 + (i as u16 * 3));
        if y + 3 > ar.height {
            break;
        }
        let r = Rect::new(x, y, w, 3);
        let (c, ic) = match n.kind {
            NotifKind::Success => (gr(a), "✓"),
            NotifKind::Error => (rd(a), "✗"),
            NotifKind::Info => (pu(a), "ℹ"),
        };
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(format!(" {} {}", ic, n.message))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(c))
                        .style(Style::default().bg(b3(a))),
                )
                .style(Style::default().fg(fg(a))),
            r,
        );
    }
}

fn pdraw(f: &mut Frame, a: &App, p: &Popup, ar: Rect) {
    match p {
        Popup::Help => {
            let r = cr(64.min(ar.width - 4), 28.min(ar.height - 4), ar);
            f.render_widget(Clear, r);
            let h = vec![
                Line::from(Span::styled("Shortcuts", Style::default().fg(or(a)).bold())),
                Line::from(""),
                hl(a, "q/Esc", "Quit"),
                hl(a, "←→/Tab", "Tabs"),
                hl(a, "1-6", "Jump"),
                hl(a, "↑↓/jk", "Select"),
                hl(a, "?", "Help"),
                hl(a, "o", "Open pos"),
                hl(a, "e", "Export"),
                hl(a, "T", "Theme"),
                hl(a, "N", "Testnet"),
                hl(a, "L", "Login"),
                hl(a, "O", "Logout"),
                hl(a, "D", "Daemon"),
                hl(a, "i", "Info"),
                hl(a, "u", "Update acct"),
                Line::from(""),
                Line::from(Span::styled(
                    "Positions:",
                    Style::default().fg(pu(a)).bold(),
                )),
                hl(a, "⏎", "Detail"),
                hl(a, "c/C", "Close/All"),
                hl(a, "s/t", "SL/TP"),
                hl(a, "m", "Margin"),
                hl(a, "$", "Cash in"),
                Line::from(""),
                Line::from(Span::styled("Orders:", Style::default().fg(pu(a)).bold())),
                hl(a, "x/X", "Cancel/All"),
                Line::from(""),
                Line::from(Span::styled("Funding:", Style::default().fg(pu(a)).bold())),
                hl(a, "d/w", "Dep/Wd⚡"),
                hl(a, "W", "On-chain"),
                hl(a, "a", "New addr"),
            ];
            f.render_widget(
                Paragraph::new(h).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .border_style(Style::default().fg(or(a)))
                        .title(Span::styled(" ⚡ Help ", Style::default().fg(or(a)).bold()))
                        .title_bottom(Line::from(Span::styled(
                            " Any key ",
                            Style::default().fg(dm(a)),
                        )))
                        .style(Style::default().bg(b3(a))),
                ),
                r,
            );
        }
        Popup::Confirm { title, message, .. } => {
            let ls: Vec<&str> = message.lines().collect();
            let r = cr(
                50.min(ar.width - 4),
                (ls.len() as u16 + 4).min(ar.height - 4),
                ar,
            );
            f.render_widget(Clear, r);
            let t: Vec<Line> = ls
                .iter()
                .map(|l| Line::from(Span::styled(*l, Style::default().fg(fg(a)))))
                .collect();
            f.render_widget(
                Paragraph::new(t)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Double)
                            .border_style(Style::default().fg(yl(a)))
                            .title(Span::styled(
                                format!(" {} ", title),
                                Style::default().fg(yl(a)).bold(),
                            ))
                            .style(Style::default().bg(b3(a))),
                    )
                    .alignment(Alignment::Center),
                r,
            );
        }
        Popup::Form {
            title,
            fields,
            active_field,
            ..
        } => {
            let h = (fields.len() as u16 * 3 + 5).min(ar.height - 4);
            let r = cr(55.min(ar.width - 4), h, ar);
            f.render_widget(Clear, r);
            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Double)
                    .border_style(Style::default().fg(or(a)))
                    .title(Span::styled(
                        format!(" {} ", title),
                        Style::default().fg(or(a)).bold(),
                    ))
                    .title_bottom(Line::from(Span::styled(
                        " Tab↑↓│Enter│Esc ",
                        Style::default().fg(dm(a)),
                    )))
                    .style(Style::default().bg(b3(a))),
                r,
            );
            let inn = Rect::new(r.x + 2, r.y + 1, r.width - 4, r.height - 2);
            for (i, fi) in fields.iter().enumerate() {
                let y = inn.y + (i as u16 * 3);
                if y + 2 > inn.y + inn.height {
                    break;
                }
                let act = i == *active_field;
                f.render_widget(
                    Paragraph::new(Span::styled(
                        &fi.label,
                        if act {
                            Style::default().fg(or(a)).bold()
                        } else {
                            Style::default().fg(dm(a))
                        },
                    )),
                    Rect::new(inn.x, y, inn.width, 1),
                );
                let d = if fi.value.is_empty() {
                    Span::styled(&fi.placeholder, Style::default().fg(Color::Rgb(60, 60, 70)))
                } else {
                    Span::styled(
                        &fi.value,
                        Style::default().fg(match fi.field_type {
                            FieldType::Number | FieldType::Price => or(a),
                            FieldType::Text => fg(a),
                        }),
                    )
                };
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw(" "),
                        d,
                        Span::styled(if act { "▌" } else { "" }, Style::default().fg(or(a))),
                    ]))
                    .style(Style::default().bg(b4(a)).fg(fg(a))),
                    Rect::new(inn.x, y + 1, inn.width, 1),
                );
                if act {
                    f.render_widget(
                        Paragraph::new(Span::styled(
                            "─".repeat(inn.width as usize),
                            Style::default().fg(or(a)),
                        )),
                        Rect::new(inn.x, y + 2, inn.width, 1),
                    );
                }
            }
        }
        Popup::Detail { title, lines } => {
            let r = cr(
                55.min(ar.width - 4),
                (lines.len() as u16 + 4).min(ar.height - 4),
                ar,
            );
            f.render_widget(Clear, r);
            let t: Vec<Line> = lines
                .iter()
                .map(|(k, v)| {
                    if k.is_empty() {
                        Line::from(Span::styled(v.as_str(), Style::default().fg(dm(a))))
                    } else {
                        Line::from(vec![
                            Span::styled(format!("  {:14}", k), Style::default().fg(dm(a))),
                            Span::styled(v.as_str(), Style::default().fg(fg(a)).bold()),
                        ])
                    }
                })
                .collect();
            f.render_widget(
                Paragraph::new(t).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .border_style(Style::default().fg(pu(a)))
                        .title(Span::styled(
                            format!(" {} ", title),
                            Style::default().fg(pu(a)).bold(),
                        ))
                        .title_bottom(Line::from(Span::styled(
                            " Any key ",
                            Style::default().fg(dm(a)),
                        )))
                        .style(Style::default().bg(b3(a))),
                ),
                r,
            );
        }
    }
}

fn pb<'b>(a: &App, t: &'b str) -> Block<'b> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 70)))
        .title(Span::styled(t, Style::default().fg(pu(a)).bold()))
        .style(Style::default().bg(b2(a)))
}
fn tw() -> [Constraint; 8] {
    [
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(8),
    ]
}
fn trow(a: &App, t: &crate::models::futures::Trade) -> Row<'static> {
    let pl = t.pl.unwrap_or(0);
    let pc = if pl >= 0 { gr(a) } else { rd(a) };
    let side = match t.side.as_str() {
        "buy" | "b" => Span::styled("▲ LONG", Style::default().fg(gr(a)).bold()),
        "sell" | "s" => Span::styled("▼ SHORT", Style::default().fg(rd(a)).bold()),
        _ => Span::styled(t.side.clone(), Style::default().fg(fg(a))),
    };
    Row::new(vec![
        Cell::from(side),
        Cell::from(format!("{}", t.quantity)),
        Cell::from(format!("{}x", t.leverage)),
        Cell::from(
            t.entry_price
                .map(|e| format!("${}", fp(e)))
                .unwrap_or_else(|| t.price.map(|p| format!("${}", fp(p))).unwrap_or("—".into())),
        ),
        Cell::from(Span::styled(
            format!("{}{}", if pl >= 0 { "+" } else { "" }, pl),
            Style::default().fg(pc).bold(),
        )),
        Cell::from(
            t.stop_loss
                .filter(|v| *v > 0.0)
                .map(|s| format!("${}", fp(s)))
                .unwrap_or("—".into()),
        ),
        Cell::from(
            t.take_profit
                .filter(|v| *v > 0.0)
                .map(|s| format!("${}", fp(s)))
                .unwrap_or("—".into()),
        ),
        Cell::from(Span::styled(
            if t.margin_type == crate::models::futures::MarginType::Cross {
                "CROSS".to_string()
            } else if t.id.len() > 8 {
                t.id[..8].to_string()
            } else {
                t.id.clone()
            },
            Style::default().fg(if t.margin_type == crate::models::futures::MarginType::Cross {
                pu(a)
            } else {
                dm(a)
            }),
        )),
    ])
}
fn hl(a: &App, k: &str, d: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:14}", k), Style::default().fg(yl(a)).bold()),
        Span::styled(d.to_string(), Style::default().fg(fg(a))),
    ])
}
fn kv<'b>(a: &App, k: &'b str, v: &str, c: Color) -> Line<'b> {
    Line::from(vec![
        Span::styled(k, Style::default().fg(dm(a))),
        Span::styled(v.to_string(), Style::default().fg(c).bold()),
    ])
}
fn fs(s: i64) -> String {
    let a = s.abs().to_string();
    let mut r = String::new();
    for (i, c) in a.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            r.push(',');
        }
        r.push(c);
    }
    let f: String = r.chars().rev().collect();
    if s < 0 {
        format!("-{}", f)
    } else {
        f
    }
}
fn fp(p: f64) -> String {
    let s = format!("{:.0}", p);
    let st = if s.starts_with('-') { 1 } else { 0 };
    let l = s.len() - st;
    let mut r = String::new();
    for (i, c) in s[st..].chars().enumerate() {
        if i > 0 && (l - i) % 3 == 0 {
            r.push(',');
        }
        r.push(c);
    }
    if st == 1 {
        format!("-{}", r)
    } else {
        r
    }
}
fn cr(w: u16, h: u16, a: Rect) -> Rect {
    Rect::new(
        a.x + (a.width.saturating_sub(w)) / 2,
        a.y + (a.height.saturating_sub(h)) / 2,
        w,
        h,
    )
}
