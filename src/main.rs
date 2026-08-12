use crate::app::{ActiveLocation, App, Fetch, Screen};
use crate::events::{Message, Request};
use anyhow::{Result, anyhow};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use std::path::Path;
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
    let (startup, state_path) = match state::path() {
        Ok(path) => {
            let (location, warning) = startup_location(&path);
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            (location, Some(path))
        }
        Err(error) => {
            eprintln!("virga: could not determine where to remember location: {error:#}");
            (ActiveLocation::default(), None)
        }
    };

    let terminal = ratatui::init();
    let mut warning = None;
    let result = run(terminal, startup, state_path.as_deref(), &mut warning);
    ratatui::restore();
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    result
}

fn startup_location(path: &Path) -> (ActiveLocation, Option<String>) {
    match state::load_from(path) {
        Ok(Some(location)) => (location, None),
        Ok(None) => (ActiveLocation::default(), None),
        Err(error) => (
            ActiveLocation::default(),
            Some(format!(
                "virga: could not load remembered location: {error:#}"
            )),
        ),
    }
}

fn accept_message(app: &mut App, message: Message, path: &Path) -> Option<String> {
    let location = app.on_message(message)?;
    state::save_to(path, location)
        .err()
        .map(|error| format!("virga: could not remember location: {error:#}"))
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
fn run(
    mut terminal: DefaultTerminal,
    startup: ActiveLocation,
    state_path: Option<&Path>,
    warning: &mut Option<String>,
) -> Result<()> {
    // Bounded: see `events::REQUEST_QUEUE`. Messages back stay unbounded — the
    // worker produces at most one per request it was handed, so bounding the
    // requests bounds the replies too, and a blocking send on the worker side
    // would be a deadlock waiting to happen.
    let (request_tx, request_rx) = mpsc::sync_channel(events::REQUEST_QUEUE);
    let (message_tx, message_rx) = mpsc::channel();
    events::spawn_worker(request_rx, message_tx);

    let mut app = App::with_location(startup);
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
            let save_warning = if let Some(path) = state_path {
                accept_message(&mut app, message, path)
            } else {
                let _ = app.on_message(message);
                None
            };
            if warning.is_none() {
                *warning = save_warning;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ActiveLocation;
    use crate::events::Message;
    use crate::weather::model::Weather;

    fn berlin() -> ActiveLocation {
        ActiveLocation {
            label: "Berlin, Germany".to_string(),
            lat: 52.52437,
            lon: 13.41053,
        }
    }

    fn loaded(request: Request) -> Message {
        let Request::Fetch { id, location } = request else {
            panic!("not a fetch")
        };
        Message::Loaded {
            id,
            location,
            weather: Weather::fixture(5, 2),
        }
    }

    #[test]
    fn remembered_location_wins_over_the_builtin_default() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        state::save_to(&path, &berlin()).unwrap();

        assert_eq!(startup_location(&path), (berlin(), None));
    }

    #[test]
    fn broken_state_falls_back_with_a_warning() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        std::fs::write(&path, "{").unwrap();

        let (location, warning) = startup_location(&path);
        assert_eq!(location, ActiveLocation::default());
        assert!(
            warning
                .unwrap()
                .contains("could not load remembered location")
        );
    }

    #[test]
    fn an_accepted_load_is_persisted() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let mut app = App::with_location(berlin());
        let message = loaded(app.initial_fetch());

        assert_eq!(accept_message(&mut app, message, &path), None);
        assert_eq!(state::load_from(&path).unwrap(), Some(berlin()));
    }

    #[test]
    fn stale_and_failed_loads_do_not_replace_state() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        state::save_to(&path, &berlin()).unwrap();
        let mut app = App::new();
        let stale = app.initial_fetch();
        let current = app.initial_fetch();

        assert_eq!(accept_message(&mut app, loaded(stale), &path), None);
        let Request::Fetch { id, .. } = current else {
            panic!("not a fetch")
        };
        assert_eq!(
            accept_message(
                &mut app,
                Message::LoadFailed {
                    id,
                    error: "offline".to_string(),
                },
                &path,
            ),
            None
        );
        assert_eq!(state::load_from(&path).unwrap(), Some(berlin()));
    }
}
