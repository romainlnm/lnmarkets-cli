mod actions;
mod app;
mod event;
mod popup;
mod ui;
use crate::api::stream::{self, StreamCredentials, StreamEvent};
use crate::api::LnmClient;
use anyhow::Result;
pub use app::App;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
pub use event::EventHandler;
use ratatui::prelude::*;
use std::io;
pub async fn run(
    refresh_secs: u64,
    client: Option<LnmClient>,
    enable_stream: bool,
    stream_creds: Option<StreamCredentials>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut o = io::stdout();
    execute!(o, EnterAlternateScreen, EnableMouseCapture)?;
    let mut t = Terminal::new(CrosstermBackend::new(o))?;
    let mut a = App::new(refresh_secs, client);
    let stream_rx = if enable_stream {
        Some(stream::start(stream_creds).events)
    } else {
        None
    };
    let mut e = EventHandler::new(refresh_secs, stream_rx);
    a.refresh_all().await;
    loop {
        t.draw(|f| ui::draw(f, &a))?;
        match e.next().await? {
            event::AppEvent::Tick => {
                a.tick_notifications();
                a.refresh_all().await;
            }
            event::AppEvent::Key(k) => {
                if a.handle_key(k).await {
                    break;
                }
            }
            event::AppEvent::Resize => {}
            event::AppEvent::Stream(StreamEvent::Status(s)) => {
                a.stream_status = s;
            }
            event::AppEvent::Stream(StreamEvent::Ticker(tk)) => {
                a.apply_stream_ticker(tk);
            }
            event::AppEvent::Stream(StreamEvent::Isolated(ev)) => {
                a.apply_isolated_event(ev);
            }
            event::AppEvent::Stream(StreamEvent::CrossOrder(ev)) => {
                a.apply_cross_order(ev);
            }
            event::AppEvent::Stream(StreamEvent::CrossPosition(u)) => {
                a.apply_cross_position(u);
            }
            event::AppEvent::Stream(StreamEvent::Deposit(d)) => {
                a.apply_deposit(d);
            }
            event::AppEvent::Stream(StreamEvent::Withdrawal(w)) => {
                a.apply_withdrawal(w);
            }
        }
    }
    disable_raw_mode()?;
    execute!(t.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    t.show_cursor()?;
    Ok(())
}
