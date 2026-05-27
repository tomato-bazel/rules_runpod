//! `View` trait — the contract between the App shell and each tab.
//!
//! Generic over a client type `C` so the framework doesn't bake in
//! any particular API surface. RunPod-specific views instantiate it
//! as `impl View<runpod::Client> for PodsView`; a different service
//! would supply its own client. The trait is intentionally small —
//! per-tab logic owns its data, renders itself, handles its own
//! keys; the App only routes input + tracks lifecycle.

use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect};
use std::process::Command;

/// Per-keypress signal from the active view back to the App.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Nothing of interest happened — keep looping.
    Continue,
    /// User triggered a manual refresh (e.g. pressed `r`).
    Refresh,
    /// View has queued an async action; App will call `run_action`
    /// and auto-refresh on success.
    Action,
    /// View has queued a foreground subprocess; App will suspend the
    /// terminal, run the command from `take_subprocess`, then restore.
    Subprocess,
    /// User wants to quit the application.
    Exit,
}

/// Pre-rendered detail-pane contents: title + ordered label/value pairs.
/// Returned by views in "detail mode" so `ui::detail` can render them
/// without knowing the resource schema.
pub struct DetailPane {
    pub title: String,
    pub pairs: Vec<(String, String)>,
}

#[async_trait]
pub trait View<C>: Send
where
    C: Send + Sync,
{
    /// Short label shown in the tab bar.
    fn title(&self) -> &str;

    /// Current row count, if any — used by App to render a "(3)" badge
    /// next to the tab title. Default `None` so non-list views can
    /// opt out.
    fn count(&self) -> Option<usize> { None }

    /// Help text rendered in the bottom status bar for this view.
    /// Default covers the universal keys; override to add per-view
    /// actions.
    fn help(&self) -> String {
        "tab: switch · ↑↓ jk: navigate · r: refresh · q: quit".into()
    }

    /// Draw into `area`. Called every tick + after every key event.
    fn render(&mut self, frame: &mut Frame, area: Rect);

    /// Handle a key press. Return `Outcome::Refresh` to ask the App
    /// to call `refresh` right after, or `Outcome::Action` to ask it
    /// to call `run_action` (then refresh on success).
    fn handle_key(&mut self, key: KeyEvent) -> Outcome;

    /// Pull the latest data from the API. Called once on startup, on
    /// every Nth tick, and after a successful action.
    async fn refresh(&mut self, client: &C) -> Result<()>;

    /// Run whatever action `handle_key` queued. Default no-op so
    /// views that have no actions don't need to override.
    async fn run_action(&mut self, _client: &C) -> Result<()> {
        Ok(())
    }

    /// Optional detail pane for the current selection. When `Some`,
    /// the App renders it in the right half of the view's area.
    /// Default `None` so views without a detail mode opt out cheaply.
    fn detail(&self) -> Option<DetailPane> { None }

    /// If `handle_key` returned `Outcome::Subprocess`, the App calls
    /// this to retrieve the foreground command to run. The view
    /// should produce + clear the pending command in one go.
    fn take_subprocess(&mut self) -> Option<Command> { None }
}
