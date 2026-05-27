//! Application shell — owns the terminal, the list of views, and the
//! event loop. Routes input to the active view, fires periodic
//! background refreshes, and handles terminal teardown on exit.
//!
//! Generic over a client type `C`. The shell never sees a concrete
//! client (it just passes `&self.client` to each view); the binary
//! at construction time decides what `C` is. For runpod-tui that's
//! `runpod::Client`. Same machinery would drive a TUI for any other
//! HTTP API.
//!
//! Lifetime of a key event:
//!   1. event task → channel → `App::run` select arm
//!   2. global keys (Tab, q, etc.) handled inline
//!   3. otherwise dispatched to the active view via View::handle_key
//!   4. on Outcome::Refresh, App fires a background refresh of that view
//!   5. on Outcome::Action, App calls view.run_action then auto-refreshes
//!   6. on Outcome::Subprocess, App suspends ratatui, runs the child,
//!      then re-enters raw mode + alternate screen on exit

use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
};
use std::io::{Stdout, stdout};
use std::time::Duration;

use crate::event::{Event, EventStream, is_exit};
use crate::ui;
use crate::view::{Outcome, View};

const TICK: Duration = Duration::from_millis(250);
const REFRESH_EVERY_TICKS: u32 = 16; // ~4 seconds between background refreshes

pub struct App<C>
where
    C: Send + Sync + 'static,
{
    client: C,
    views: Vec<Box<dyn View<C>>>,
    active: usize,
    status: Option<String>,
    /// Tick counter for time-based refresh cadence.
    ticks_since_refresh: u32,
}

impl<C> App<C>
where
    C: Send + Sync + 'static,
{
    pub fn new(client: C, views: Vec<Box<dyn View<C>>>) -> Self {
        Self {
            client,
            views,
            active: 0,
            status: None,
            ticks_since_refresh: 0,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = enter_terminal()?;
        let result = self.event_loop(&mut terminal).await;
        // Always restore the terminal, even if the loop errored.
        leave_terminal(&mut terminal).ok();
        result
    }

    async fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        // Initial refresh of every view so we have data on first paint.
        for i in 0..self.views.len() {
            if let Err(e) = self.views[i].refresh(&self.client).await {
                self.status = Some(format!("error: {e}"));
            }
        }

        let mut events = EventStream::spawn(TICK);
        loop {
            let help = self.views[self.active].help();
            let detail = self.views[self.active].detail();
            terminal.draw(|frame| {
                let area = frame.area();
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(2),  // tab bar
                        Constraint::Min(0),     // active view
                        Constraint::Length(1),  // status bar
                    ])
                    .split(area);
                let titles: Vec<String> = self
                    .views
                    .iter()
                    .map(|v| match v.count() {
                        Some(n) => format!("{} ({n})", v.title()),
                        None => v.title().into(),
                    })
                    .collect();
                let title_refs: Vec<&str> = titles.iter().map(|s| s.as_str()).collect();
                ui::tabs::render(frame, layout[0], &title_refs, self.active);

                if let Some(d) = &detail {
                    let split = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                        .split(layout[1]);
                    self.views[self.active].render(frame, split[0]);
                    ui::detail::render(frame, split[1], &d.title, &d.pairs);
                } else {
                    self.views[self.active].render(frame, layout[1]);
                }

                ui::status_line::render(
                    frame,
                    layout[2],
                    &help,
                    self.status.as_deref(),
                );
            })?;

            let Some(ev) = events.next().await else { break };
            match ev {
                Event::Key(key) => match self.handle_global(key)? {
                    GlobalAction::Exit => break,
                    GlobalAction::Pass => self.dispatch_to_view(terminal, key).await,
                    GlobalAction::Continue => {}
                },
                Event::Tick => {
                    self.ticks_since_refresh = self.ticks_since_refresh.wrapping_add(1);
                    if self.ticks_since_refresh >= REFRESH_EVERY_TICKS {
                        self.ticks_since_refresh = 0;
                        self.refresh_active().await;
                    }
                }
                Event::Resize(_, _) => { /* next draw handles it */ }
            }
        }
        Ok(())
    }

    fn handle_global(&mut self, key: KeyEvent) -> Result<GlobalAction> {
        if is_exit(key)? {
            return Ok(GlobalAction::Exit);
        }
        match (key.code, key.modifiers) {
            (KeyCode::Tab, _) => {
                self.active = (self.active + 1) % self.views.len();
                Ok(GlobalAction::Continue)
            }
            (KeyCode::BackTab, _) => {
                self.active = (self.active + self.views.len() - 1) % self.views.len();
                Ok(GlobalAction::Continue)
            }
            // Number keys 1..=9 select tabs directly.
            (KeyCode::Char(c), KeyModifiers::NONE) if c.is_ascii_digit() && c != '0' => {
                let i = (c as usize) - ('1' as usize);
                if i < self.views.len() {
                    self.active = i;
                }
                Ok(GlobalAction::Continue)
            }
            _ => Ok(GlobalAction::Pass),
        }
    }

    async fn dispatch_to_view(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        key: KeyEvent,
    ) {
        match self.views[self.active].handle_key(key) {
            Outcome::Continue => {}
            Outcome::Exit => { /* views can't unilaterally exit yet */ }
            Outcome::Refresh => {
                self.refresh_active().await;
            }
            Outcome::Action => {
                self.status = Some("…".into());
                match self.views[self.active].run_action(&self.client).await {
                    Ok(()) => {
                        self.refresh_active().await;
                    }
                    Err(e) => {
                        self.status = Some(format!("error: {e}"));
                    }
                }
            }
            Outcome::Subprocess => {
                if let Some(mut cmd) = self.views[self.active].take_subprocess() {
                    let result = run_foreground(terminal, &mut cmd);
                    match result {
                        Ok(status) if status.success() => self.status = None,
                        Ok(status) => self.status = Some(format!("exit {}", status)),
                        Err(e) => self.status = Some(format!("error: {e}")),
                    }
                }
            }
        }
    }

    async fn refresh_active(&mut self) {
        self.status = Some("loading…".into());
        match self.views[self.active].refresh(&self.client).await {
            Ok(()) => self.status = None,
            Err(e) => self.status = Some(format!("error: {e}")),
        }
    }
}

enum GlobalAction {
    /// Quit the application.
    Exit,
    /// Key was handled at the App level; do not dispatch to the view.
    Continue,
    /// Key was not interesting to the App; pass it to the active view.
    Pass,
}

fn enter_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(out))?)
}

fn leave_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Suspend the ratatui chrome, run `cmd` in the foreground (inheriting
/// stdin/out/err so the child can drive the terminal — e.g. an
/// interactive ssh session), then restore raw mode + alternate screen.
fn run_foreground(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    cmd: &mut std::process::Command,
) -> Result<std::process::ExitStatus> {
    leave_terminal(terminal)?;
    let status_result = cmd.status();
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(status_result?)
}
