use crate::app::{App, Fetch, Screen};
use crate::events::{Message, Request};
use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use std::sync::mpsc;
use std::time::Duration;

mod app;
mod events;
mod ui;
mod units;
mod weather;

struct Place {
    name: &'static str,
    lat: f64,
    lon: f64,
}

const DEFAULT_LOCATION: Place = Place {
    name: "Frederick, Maryland, United States",
    lat: 39.41427,
    lon: -77.41054,
};

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal) -> Result<()> {
    let (request_tx, request_rx) = mpsc::channel();
    let (message_tx, message_rx) = mpsc::channel();
    events::spawn_worker(request_rx, message_tx);

    request_tx.send(Request::Fetch {
        lat: DEFAULT_LOCATION.lat,
        lon: DEFAULT_LOCATION.lon,
    })?;

    let mut app = App::new();
    app.location = Some(DEFAULT_LOCATION.name.to_string());

    let mut dirty = true;

    loop {
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
            match message {
                Message::Loaded(w) => app.weather = Fetch::Ready(w),
                Message::LoadFailed(e) => app.weather = Fetch::Failed(e),
                Message::Located(l) => app.results = Fetch::Ready(l),
                Message::SearchFailed(e) => app.results = Fetch::Failed(e),
            }
        }

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                break Ok(());
            }

            dirty = true;

            match app.screen {
                Screen::Weather => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    KeyCode::Char('r') => {
                        app.weather = Fetch::Loading;
                        request_tx.send(Request::Fetch {
                            lat: DEFAULT_LOCATION.lat,
                            lon: DEFAULT_LOCATION.lon,
                        })?;
                    }
                    KeyCode::Char('t') => {
                        app.unit = app.unit.toggle();
                    }
                    KeyCode::Char('l') => {
                        app.screen = Screen::Search;
                        app.query.clear();
                    }
                    _ => {}
                },
                Screen::Search => match key.code {
                    KeyCode::Esc => app.screen = Screen::Weather,
                    KeyCode::Backspace => {
                        app.query.pop();
                        app.invalidate_results();
                    }
                    KeyCode::Enter => {
                        let picked = match &app.results {
                            Fetch::Ready(locations) => locations
                                .get(app.selected)
                                .map(|l| (l.lat, l.lon, l.label())),
                            _ => None,
                        };

                        if let Some((lat, lon, label)) = picked {
                            app.weather = Fetch::Loading;
                            app.results = Fetch::Idle;
                            app.screen = Screen::Weather;
                            app.location = Some(label);
                            request_tx.send(Request::Fetch { lat, lon })?;
                        } else if !app.query.is_empty() {
                            app.results = Fetch::Loading;
                            app.selected = 0;
                            request_tx.send(Request::Search(app.query.clone()))?;
                        }
                    }
                    KeyCode::Char(c) => {
                        app.query.push(c);
                        app.invalidate_results();
                    }
                    KeyCode::Up => app.selected = app.selected.saturating_sub(1),
                    KeyCode::Down => {
                        if let Fetch::Ready(locations) = &app.results
                            && app.selected + 1 < locations.len()
                        {
                            app.selected += 1;
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}
