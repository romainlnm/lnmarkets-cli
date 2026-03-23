mod actions;
mod app;
mod event;
mod popup;
mod ui;
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
pub async fn run(refresh_secs: u64, client: Option<LnmClient>) -> Result<()> {
    enable_raw_mode()?;
    let mut o = io::stdout();
    execute!(o, EnterAlternateScreen, EnableMouseCapture)?;
    let mut t = Terminal::new(CrosstermBackend::new(o))?;
    let mut a = App::new(refresh_secs, client);
    let mut e = EventHandler::new(refresh_secs);
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
        }
    }
    disable_raw_mode()?;
    execute!(t.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    t.show_cursor()?;
    Ok(())
}
