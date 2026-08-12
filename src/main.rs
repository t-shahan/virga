use crate::app::{App, Fetch, Screen};
use crate::events::Request;
use anyhow::{Result, anyhow};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::Duration;

mod app;
mod events;
mod input;
mod state;
mod ui;
mod units;
mod weather;

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

/// Hand a request to the worker without ever blocking the draw loop.
///
/// `send` on a bounded channel parks the caller until a slot frees — and the
/// caller here is the thread that owns the terminal, so the app would stop
/// drawing and stop reading keys until the network answered. `try_send` keeps
/// the loop turning and hands the refusal back to `App`, which owns what the
/// user is told.
fn dispatch(tx: &SyncSender<Request>, app: &mut App, request: Request) -> Result<()> {
    match tx.try_send(request) {
        Ok(()) => Ok(()),
        // Unreachable while the guards in `App` hold — the queue is deeper
        // than it can fill. It is handled anyway so that the day a guard is
        // dropped, the screen says something instead of waiting forever on a
        // request that was never sent.
        Err(TrySendError::Full(request)) => {
            app.on_dispatch_dropped(request);
            Ok(())
        }
        Err(TrySendError::Disconnected(_)) => Err(anyhow!("the worker thread has stopped")),
    }
}

/// Terminal setup, the draw loop, and carrying messages between the worker and
/// the app. Every state transition lives in `App` and every decision about what
/// a key means lives in `input`, so what remains here is the part that
/// genuinely needs a terminal and a channel.
fn run(mut terminal: DefaultTerminal) -> Result<()> {
    // Bounded: see `events::REQUEST_QUEUE`. Messages back stay unbounded — the
    // worker produces at most one per request it was handed, so bounding the
    // requests bounds the replies too, and a blocking send on the worker side
    // would be a deadlock waiting to happen.
    let (request_tx, request_rx) = mpsc::sync_channel(events::REQUEST_QUEUE);
    let (message_tx, message_rx) = mpsc::channel();
    events::spawn_worker(request_rx, message_tx);

    let mut app = App::new();
    let initial = app.initial_fetch();
    dispatch(&request_tx, &mut app, initial)?;

    let mut dirty = true;
    let mut last_size = terminal.size()?;

    loop {
        // Belt and braces against the class of bug that made resize freeze the
        // app: ratatui only reconciles its buffer inside draw(), so if the size
        // changes and nothing marks the frame dirty, a stale layout persists
        // forever. Polling it costs one ioctl per tick and does not rely on an
        // event turning up.
        let size = terminal.size()?;
        if size != last_size {
            last_size = size;
            dirty = true;
        }

        // Only the spinner and the search cursor change on their own. With
        // neither on screen there is nothing to redraw until input or a worker
        // message arrives, so an idle app costs no CPU instead of ten frames a
        // second.
        let animating = matches!(app.weather, Fetch::Loading)
            || matches!(app.results, Fetch::Loading)
            || matches!(app.screen, Screen::Search);

        if dirty || animating {
            app.tick = app.tick.wrapping_add(1);
            terminal.draw(|frame| ui::render(frame, &app))?;
            dirty = false;
        }

        while let Ok(message) = message_rx.try_recv() {
            dirty = true;
            app.on_message(message);
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                // A resize invalidates the whole buffer. The old loop redrew ten
                // times a second and papered over this; now that it only draws on
                // change, the resize has to say so itself.
                Event::Resize(_, _) => dirty = true,
                Event::Key(key) => {
                    // Keys that mean nothing on this screen — and every key
                    // release — leave no mark, so they do not even cost a redraw.
                    if let Some(action) = input::action_for(key, app.screen) {
                        dirty = true;

                        if let Some(request) = app.on_action(action) {
                            dispatch(&request_tx, &mut app, request)?;
                        }
                        if app.should_quit {
                            break Ok(());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
