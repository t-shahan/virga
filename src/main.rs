use crate::app::{App, Fetch, Screen};
use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use std::sync::mpsc;
use std::time::Duration;

mod app;
mod events;
mod input;
mod ui;
mod units;
mod weather;

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

/// Terminal setup, the draw loop, and carrying messages between the worker and
/// the app. Every state transition lives in `App` and every decision about what
/// a key means lives in `input`, so what remains here is the part that
/// genuinely needs a terminal and a channel.
fn run(mut terminal: DefaultTerminal) -> Result<()> {
    let (request_tx, request_rx) = mpsc::channel();
    let (message_tx, message_rx) = mpsc::channel();
    events::spawn_worker(request_rx, message_tx);

    let mut app = App::new();
    request_tx.send(app.initial_fetch())?;

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
                            request_tx.send(request)?;
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
