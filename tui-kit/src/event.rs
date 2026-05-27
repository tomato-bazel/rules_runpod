//! Unified event stream — input + periodic tick.
//!
//! Spawning one tokio task per source keeps the main loop a single
//! `tokio::select!` instead of polling crossterm directly (which
//! blocks). The two channels are bounded so a slow main loop can't
//! cause unbounded backpressure.

use anyhow::Result;
use crossterm::event::{Event as CtEvent, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    /// Fires every `tick_interval` so views can re-render and
    /// `App` can decide whether to refresh data.
    Tick,
    /// Terminal was resized — caller should redraw on next tick.
    Resize(u16, u16),
}

pub struct EventStream {
    rx: mpsc::Receiver<Event>,
}

impl EventStream {
    pub fn spawn(tick_interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel(64);

        // Input task. crossterm::event::read is blocking, so we use
        // its async cousin via spawn_blocking + a manual loop.
        let input_tx = tx.clone();
        tokio::task::spawn_blocking(move || {
            loop {
                match crossterm::event::read() {
                    Ok(CtEvent::Key(k)) => {
                        if input_tx.blocking_send(Event::Key(k)).is_err() {
                            break;
                        }
                    }
                    Ok(CtEvent::Resize(w, h)) => {
                        if input_tx.blocking_send(Event::Resize(w, h)).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {} // ignore mouse / focus / paste for now
                    Err(_) => break,
                }
            }
        });

        // Tick task.
        let tick_tx = tx;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if tick_tx.send(Event::Tick).await.is_err() {
                    break;
                }
            }
        });

        Self { rx }
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}

/// Convenience shorthand for the common case of "is this Ctrl-C / Esc".
pub fn is_exit(key: KeyEvent) -> Result<bool> {
    use crossterm::event::{KeyCode, KeyModifiers};
    Ok(match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
        (KeyCode::Char('q'), KeyModifiers::NONE) => true,
        (KeyCode::Esc, _) => true,
        _ => false,
    })
}
