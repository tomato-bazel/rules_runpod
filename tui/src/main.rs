//! runpod-tui — multi-pane terminal UI for managing RunPod resources.
//!
//! Composition layer: picks the concrete client (runpod::Client),
//! builds the resource views, hands them to `tui_kit::App`. All
//! generic TUI machinery lives in the `tui-kit` crate.

mod config;
mod views;

use anyhow::Result;
use tui_kit::{App, View};

#[tokio::main]
async fn main() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(_) | Err(dotenvy::Error::Io(_)) => {}
        Err(e) => eprintln!("[runpod-tui] .env load failed: {e}"),
    }
    let cfg = config::Config::load()?;
    let client = runpod::Client::new(cfg.api_key)?;

    let views: Vec<Box<dyn View<runpod::Client>>> = vec![
        Box::new(views::pods::PodsView::new()),
        Box::new(views::templates::TemplatesView::new()),
        Box::new(views::volumes::VolumesView::new()),
    ];
    let mut app = App::new(client, views);
    app.run().await
}
