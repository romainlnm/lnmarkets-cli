use crate::api::stream::StreamEvent;
use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;

pub enum AppEvent {
    Tick,
    Key(KeyEvent),
    Resize,
    Stream(StreamEvent),
}

pub struct EventHandler {
    tick_rate: Duration,
    stream_rx: Option<mpsc::UnboundedReceiver<StreamEvent>>,
}

impl EventHandler {
    pub fn new(s: u64, stream_rx: Option<mpsc::UnboundedReceiver<StreamEvent>>) -> Self {
        Self {
            tick_rate: Duration::from_secs(s),
            stream_rx,
        }
    }

    pub async fn next(&mut self) -> Result<AppEvent> {
        let mut tick = interval(self.tick_rate);
        // First call resolves immediately — skip it to align with the original behavior.
        tick.tick().await;

        loop {
            // Build a future for the optional stream receiver. When there's no
            // stream, fall back to a pending future so the select branch never fires.
            let stream_fut = async {
                match self.stream_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending::<Option<StreamEvent>>().await,
                }
            };

            tokio::select! {
                _ = tick.tick() => return Ok(AppEvent::Tick),
                ev = stream_fut => {
                    if let Some(e) = ev {
                        return Ok(AppEvent::Stream(e));
                    }
                    // Stream channel closed — drop the receiver and continue.
                    self.stream_rx = None;
                }
                r = tokio::task::spawn_blocking(|| {
                    if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                        Some(event::read())
                    } else {
                        None
                    }
                }) => {
                    if let Ok(Some(Ok(e))) = r {
                        return match e {
                            Event::Key(k) => Ok(AppEvent::Key(k)),
                            Event::Resize(_, _) => Ok(AppEvent::Resize),
                            _ => continue,
                        };
                    }
                }
            }
        }
    }
}
