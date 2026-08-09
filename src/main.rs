use std::sync::mpsc;
use std::time::Duration;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::crossterm::event;
use ratatui::layout::Flex;
use ratatui::{DefaultTerminal, Frame};
use ratatui::widgets::Clear;
use ratatui::layout::Alignment;
use ratatui::style::Stylize;
use ratatui::text::Line;
use anyhow::Result;
use crate::app::App;
use crate::app::Screen;
use crate::units::Unit;
use crate::weather::model::Weather;
use crate::events::{Message, Request};
use crate::weather::code::aqi_label;
use crate::weather::code::{description, emoji};
use crate::app::Fetch;
use crate::weather::model::Location;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph};
mod weather;
mod units;
mod app;
mod events;

const DEFAULT_LOCATION: (f64, f64) = (39.4143, -77.4105);

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
        lat: DEFAULT_LOCATION.0,
        lon: DEFAULT_LOCATION.1,
    })?;

    let mut app = App::new();

    loop {
        app.tick = app.tick.wrapping_add(1);
        terminal.draw(|frame| render(frame, &app))?;

        while let Ok(message) = message_rx.try_recv() {
            match message {
                Message::Loaded(w) => app.weather = Fetch::Ready(w),
                Message::LoadFailed(e) => app.weather = Fetch::Failed(e),
                Message::Located(l) => app.results = Fetch::Ready(l),
                Message::SearchFailed(e) => app.results = Fetch::Failed(e),
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break Ok(());
                }
                match app.screen {
                    Screen::Weather => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                        KeyCode::Char('r') => {
                            app.weather = Fetch::Loading;
                            request_tx.send(Request::Fetch {
                                lat: DEFAULT_LOCATION.0,
                                lon: DEFAULT_LOCATION.1,
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
                        KeyCode::Backspace => { app.query.pop(); }
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
                        KeyCode::Char(c) => { app.query.push(c); }
                        KeyCode::Up => app.selected = app.selected.saturating_sub(1),
                        KeyCode::Down => {
                            if let Fetch::Ready(locations) = &app.results {
                                if app.selected + 1 < locations.len() {
                                    app.selected += 1;
                                }
                            }
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let [top_area, current_area, forecast_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(7),
        Constraint::Fill(1)
    ])
    .areas(area);

    match app.screen {
        Screen::Weather => match &app.weather {
            Fetch::Ready(w) => {
                top_area_render(frame, app, w, top_area);
                current_area_render(frame, w, current_area, app.unit);
                forecast_area_render(frame, w, forecast_area, app.unit);
            }
            Fetch::Loading => popup_render(
                frame,
                area,
                "Loading",
                &format!("{} fetching...", spinner(app.tick)),
            ),
            Fetch::Failed(msg) => popup_render(
                frame,
                area,
                "Error",
                msg,
            ),
            Fetch::Idle => {}
        },
        Screen::Search => search_render(frame, app, area),
    }
}

fn search_render(frame: &mut Frame, app: &App, area: Rect) {
    let area = centered(area, 50, 12);
    frame.render_widget(Clear, area);

    let block = Block::bordered().title("Search City (Enter to search, Esc to cancel");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [input_area, list_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    frame.render_widget(Paragraph::new(app.query.as_str()).yellow(), input_area);

    let body: Vec<Line> = match &app.results {
        Fetch::Idle => Vec::new(),
        Fetch::Loading => vec![Line::from(format!("{} searching...", spinner(app.tick))).dim()],
        Fetch::Ready(locations) if locations.is_empty() => {
            vec![Line::from("no matches").dim()]
        }
        Fetch::Ready(locations) => locations
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let marker = if i == app.selected { ">" } else { " " };
                let text = format!("{marker} {}", l.label());
                if i == app.selected {
                    Line::from(text).yellow().bold()
                } else {
                    Line::from(text).cyan()
                }
            })
            .collect(),
        Fetch::Failed(e) => vec![Line::from(format!("error: {e}")).red()],
    };

    frame.render_widget(Paragraph::new(body), list_area);
}

fn top_area_render(frame: &mut Frame, app: &App, weather: &Weather, area: Rect) {
    let block = Block::bordered();
    let inner = block.inner(area);
    let name = app.location.as_deref().unwrap_or(&weather.location);
    frame.render_widget(block, area);

    let [name_area, emoji_area] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(5),
    ])
    .areas(inner);

    frame.render_widget(Paragraph::new(name), name_area);
    frame.render_widget(Paragraph::new(emoji(weather.current.code)), emoji_area);

}

fn current_area_render(frame: &mut Frame, weather: &Weather, area: Rect, unit: Unit) {
    let aqi = match &weather.air_quality {
        Some(aq) => format!("AQI: {} - {}", aq.us_aqi, aqi_label(aq.us_aqi)),
        None => "AQI: unavailable".to_string(),
    };

    let current = Paragraph::new(format!(
        "{:.1}{} (feels like {:.1}{})\nwind {:.0} {}\n{}\n{}",
            unit.temp(weather.current.temp_c),
            unit.temp_symbol(),
            unit.temp(weather.current.feels_like_c),
            unit.temp_symbol(),
            unit.speed(weather.current.wind_kph),
            unit.speed_label(),
            description(weather.current.code),
            aqi,
    ))
    .block(Block::bordered().title("Now"));

    frame.render_widget(current, area);
}

fn forecast_area_render(frame: &mut Frame, weather: &Weather, area: Rect, unit: Unit) {
    let lines = weather
        .daily
        .iter()
        .map(|d| format!("{}    {:.0}{} / {:.0}{}", d.date, unit.temp(d.high_c), unit.temp_symbol(), unit.temp(d.low_c), unit.temp_symbol()))
        .collect::<Vec<_>>();

    let forecast = Paragraph::new(lines.join("\n"))
        .block(Block::bordered().title("Forecast"));

    frame.render_widget(forecast, area);
}

fn keybind_legend_render(frame: &mut Frame, area: Rect, unit: Unit) {
    todo!("Add a small section at the bottom with quick keybind help, no title, just something like 'quit' with q in a different color than the rest of the word and similar style for the other commands. No border or background color. Should be modern and stylish.")
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}

fn popup_render(frame: &mut Frame, area: Rect, title: &str, body: &str) {
    let area = centered(area, 40, 5);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Center)
            .block(Block::bordered().title(title)),
        area,
    )
}

fn spinner(tick: usize) -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    FRAMES[(tick / 2) % FRAMES.len()]
}
