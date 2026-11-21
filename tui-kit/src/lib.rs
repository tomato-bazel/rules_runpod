//! tui-kit — a small framework for building multi-pane ratatui apps
//! backed by an async HTTP client. Generic over the client type so
//! the same machinery drives a TUI for any service that has typed
//! list/get/create/delete operations.
//!
//! # Architecture
//!
//! Three layers, top-to-bottom:
//!
//! 1. **`View<C>`** — per-tab contract. Each tab implements `View<C>`
//!    where `C` is the user's client type. Owns its data, renders
//!    itself, handles its own keys. Sees the client only via async
//!    `refresh` / `run_action` hooks.
//! 2. **`App<C>`** — application shell. Owns the terminal, holds a
//!    `Vec<Box<dyn View<C>>>`, runs the event loop, routes input,
//!    fires periodic refreshes. Knows nothing about any particular
//!    resource.
//! 3. **`ui::*`** — reusable widgets: selectable table, tab bar,
//!    confirm modal, generic form, detail pane, status line. None
//!    of these reference the client type or the View trait — they're
//!    pure rendering helpers usable from any ratatui app.
//!
//! # Building an app
//!
//! ```no_run
//! # use anyhow::Result;
//! # use async_trait::async_trait;
//! # use crossterm::event::KeyEvent;
//! # use ratatui::{Frame, layout::Rect};
//! # use tui_kit::{App, View, Outcome};
//! #
//! struct MyClient;
//!
//! struct PodsView;
//! #[async_trait]
//! impl View<MyClient> for PodsView {
//!     fn title(&self) -> &str { "Pods" }
//!     fn render(&mut self, _frame: &mut Frame, _area: Rect) {}
//!     fn handle_key(&mut self, _key: KeyEvent) -> Outcome { Outcome::Continue }
//!     async fn refresh(&mut self, _client: &MyClient) -> Result<()> { Ok(()) }
//! }
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let client = MyClient;
//! let views: Vec<Box<dyn View<MyClient>>> = vec![Box::new(PodsView)];
//! App::new(client, views).run().await
//! # }
//! ```
//!
//! # Lift path
//!
//! Currently shipped inline next to `runpod-tui` in the same
//! workspace. The crate has no RunPod dependencies — moving it
//! to its own repo is a `git mv` +
//! adding it to a Bazel registry.

pub mod app;
pub mod event;
pub mod theme;
pub mod ui;
pub mod view;

pub use app::App;
pub use view::{DetailPane, Outcome, View};
