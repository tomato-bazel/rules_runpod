# tui-kit

A small framework for building multi-pane [ratatui](https://github.com/ratatui-org/ratatui)
apps backed by an async HTTP client. Generic over the client type so
the same machinery drives a TUI for any service with typed
list/get/create/delete operations.

## Status

v0.1, ships with [runpod-tui](../tui). Designed to be extractable
into its own repo (e.g. `fastverk/tui-kit`) with `git mv` + a
fastverk-registry entry. The crate has no dependencies outside this
workspace beyond `ratatui`, `crossterm`, `tokio`, `anyhow`, and
`async-trait`.

## What's in the box

```
src/
├── app.rs        // App<C>: terminal owner + event loop
├── event.rs      // unified input + tick channel
├── theme.rs      // single source of truth for colors
├── view.rs       // View<C> trait + Outcome enum + DetailPane
└── ui/
    ├── tabs.rs           tab bar renderer
    ├── table.rs          SelectableTable<R: Row>
    ├── modal.rs          centred confirm modal
    ├── form.rs           Form with TextField + ChoiceField
    ├── detail.rs         key-value detail pane
    └── status_line.rs    bottom help + status text
```

Nothing here imports any HTTP client. Everything is generic over
the consumer-supplied `C` type.

## Building an app

```rust
use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect};
use tui_kit::{App, Outcome, View};

struct MyClient { /* … */ }

struct PodsView { /* … */ }

#[async_trait]
impl View<MyClient> for PodsView {
    fn title(&self) -> &str { "Pods" }
    fn render(&mut self, frame: &mut Frame, area: Rect) { /* … */ }
    fn handle_key(&mut self, key: KeyEvent) -> Outcome { /* … */ }
    async fn refresh(&mut self, client: &MyClient) -> Result<()> { /* … */ }
    async fn run_action(&mut self, client: &MyClient) -> Result<()> { /* … */ }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = MyClient { /* … */ };
    let views: Vec<Box<dyn View<MyClient>>> = vec![Box::new(PodsView { /* … */ })];
    App::new(client, views).run().await
}
```

The [runpod-tui crate](../tui/src) is a complete, working example —
look at `views/pods.rs` for a substantial `View<C>` impl that
includes table navigation, a confirm modal, a create form, a detail
pane, and a foreground subprocess (for ssh).

## Outcomes

A `View<C>::handle_key` returns one of:

| Outcome | Meaning |
|---|---|
| `Continue` | nothing happened; keep looping |
| `Refresh` | call `refresh(&client)` immediately |
| `Action` | call `run_action(&client)`, then auto-refresh on success |
| `Subprocess` | call `take_subprocess() -> Command`, suspend the TUI, run it foreground, restore on exit |
| `Exit` | reserved for future use |

## Reusable widgets

The widgets in `ui/` are usable from outside the `View` trait —
they're stateless render helpers (with one stateful exception,
`SelectableTable<R>`, which owns selection + scroll state).

| Module | Function |
|---|---|
| `tabs::render(frame, area, titles, active)` | tab bar |
| `table::SelectableTable<R>` | scroll + ↑↓ jk navigation |
| `modal::render_confirm(frame, area, prompt)` | centred y/n prompt |
| `form::Form` + `Field::Text` / `Field::Choice` | data-entry form |
| `detail::render(frame, area, title, pairs)` | k/v side-pane |
| `status_line::render(frame, area, help, status)` | bottom bar |

## Lifting to its own repo

When extraction makes sense (when a second consumer wants the same
machinery):

1. `git mv tools/runpod-cli/tui-kit /tmp/tui-kit && cd /tmp/tui-kit && git init`
2. Add a `metadata.json` + `MODULE.bazel` to `fastverk/bazel-registry/modules/tui-kit/0.1.0/`
3. Push the repo to `fastverk/tui-kit`
4. In runpod-cli, replace the path dep with `tui-kit = "0.1"` and
   the Bazel target with the registry-supplied `@tui_kit//:tui-kit`
