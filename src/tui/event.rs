use anyhow::Result; use crossterm::event::{self,Event,KeyEvent}; use std::time::Duration; use tokio::time::interval;
pub enum AppEvent{Tick,Key(KeyEvent),Resize}
pub struct EventHandler{tick_rate:Duration}
impl EventHandler{
    pub fn new(s:u64)->Self{Self{tick_rate:Duration::from_secs(s)}}
    pub async fn next(&mut self)->Result<AppEvent>{
        let mut t=interval(self.tick_rate);t.tick().await;
        loop{tokio::select!{_=t.tick()=>return Ok(AppEvent::Tick),
            r=tokio::task::spawn_blocking(||{if event::poll(Duration::from_millis(50)).unwrap_or(false){Some(event::read())}else{None}})=>{
                if let Ok(Some(Ok(e)))=r{return match e{Event::Key(k)=>Ok(AppEvent::Key(k)),Event::Resize(_,_)=>Ok(AppEvent::Resize),_=>continue};}}}}
    }
}
