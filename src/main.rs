use crate::app::App;
use crate::app::Fetch;
use crate::app::Screen;
use crate::events::{Message, Request};
use crate::units::Unit;
use crate::weather::code::aqi_label;
use crate::weather::code::{description, emoji};
use crate::weather::model::Location;
use crate::weather::model::Weather;
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use ratatui::crossterm::event;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{self, SetSize};
use ratatui::layout::Alignment;
use ratatui::layout::Flex;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Paragraph};
use ratatui::widgets::{Borders, Clear};
use ratatui::{DefaultTerminal, Frame};
use std::io::stdout;
use std::sync::mpsc;
use std::time::Duration;
mod app;
mod events;
mod units;
mod weather;

struct Place {
    name: &'static str,
    lat: f64,
    lon: f64,
}

const DEFAULT_LOCATION: Place = Place {
    name: "Frederick, Maryland",
    lat: 39.41427,
    lon: -77.41054,
};

/// The layout is tuned for this size. Terminals may refuse the resize — window
/// manipulation is commonly disabled — so this is a request, not a guarantee.
const WINDOW_COLS: u16 = 100;
const WINDOW_ROWS: u16 = 30;

fn main() -> Result<()> {
    let original_size = terminal::size().ok();
    let _ = execute!(stdout(), SetSize(WINDOW_COLS, WINDOW_ROWS));

    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();

    // Leave the terminal the size we found it.
    if let Some((cols, rows)) = original_size {
        let _ = execute!(stdout(), SetSize(cols, rows));
    }

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

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                break Ok(());
            }

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

fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let [top_area, current_area, forecast_area, legend_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(7),
        Constraint::Fill(1),
        Constraint::Length(1),
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
            Fetch::Failed(msg) => popup_render(frame, area, "Error", msg),
            Fetch::Idle => {}
        },
        Screen::Search => search_render(frame, app, area),
    }
    keybind_legend_render(frame, app, legend_area);
}

fn search_render(frame: &mut Frame, app: &App, area: Rect) {
    let area = centered(area, 50, 12);
    frame.render_widget(Clear, area);

    let block = Block::bordered().title("Search");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [input_area, list_area] = Layout::vertical([
        Constraint::Length(2), // text row + underline
        Constraint::Fill(1),
    ])
    .areas(inner);

    let input_block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(Color::DarkGray));
    let input_inner = input_block.inner(input_area);
    frame.render_widget(input_block, input_area);

    let input_line = if app.query.is_empty() {
        Line::from(vec![
            Span::from("❯ ").cyan(),
            Span::from("search for a city").dark_gray().italic(),
        ])
    } else {
        let cursor = if (app.tick / 5).is_multiple_of(2) {
            "▏"
        } else {
            " "
        };
        Line::from(vec![
            Span::from("❯ ").cyan(),
            Span::from(app.query.as_str()).white(),
            Span::from(cursor).cyan(),
        ])
    };

    frame.render_widget(Paragraph::new(input_line), input_inner);

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
    frame.render_widget(block, area);

    let name = app.location.as_deref().unwrap_or(&weather.location);

    let lines = vec![
        Line::from(name.to_uppercase()).bold().cyan(),
        Line::from(weather.current.code.map_or(UNKNOWN, description)).dark_gray(),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

fn current_area_render(frame: &mut Frame, weather: &Weather, area: Rect, unit: Unit) {
    let block = Block::bordered().title("Now");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Centre the hero and details as one group rather than pinning them left.
    let [content] = Layout::horizontal([Constraint::Length(HERO_WIDTH + DETAIL_WIDTH)])
        .flex(Flex::Center)
        .areas(inner);

    let [hero_area, detail_area] = Layout::horizontal([
        Constraint::Length(HERO_WIDTH),
        Constraint::Length(DETAIL_WIDTH),
    ])
    .areas(content);

    // The block font already has a '-' glyph, so a missing reading renders as
    // "--" at the same scale rather than collapsing the layout.
    let temp = weather
        .current
        .temp_c
        .map_or_else(|| "--".to_string(), |c| format!("{:.0}", unit.temp(c)));

    // Alignment::Center centres each line on its own width, so the row carrying
    // the unit symbol would sit offset from the rest. Pad the others to match.
    let symbol = unit.temp_symbol();
    let symbol_pad = " ".repeat(symbol.chars().count());

    let hero: Vec<Line> = big_digits(&temp)
        .iter()
        .enumerate()
        .map(|(i, row)| {
            // Hang the unit symbol off the middle row so it sits centred against the digits.
            if i == DIGIT_ROWS / 2 {
                Line::from(vec![
                    Span::from(row.clone()).bold().cyan(),
                    Span::from(symbol.to_string()).cyan(),
                ])
            } else {
                Line::from(format!("{row}{symbol_pad}")).bold().cyan()
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(hero).alignment(Alignment::Center), hero_area);

    let aqi = match &weather.air_quality {
        Some(aq) => format!("{} · {}", aq.us_aqi, aqi_label(aq.us_aqi)),
        None => "unavailable".to_string(),
    };

    let feels_like = weather.current.feels_like_c.map_or_else(
        || UNKNOWN.to_string(),
        |c| format!("{:.0}{}", unit.temp(c), unit.temp_symbol()),
    );

    let wind = weather.current.wind_kph.map_or_else(
        || UNKNOWN.to_string(),
        |kph| format!("{:.0} {}", unit.speed(kph), unit.speed_label()),
    );

    let details = vec![
        Line::from(""),
        detail_line("feels like", &feels_like),
        detail_line("wind", &wind),
        detail_line("air quality", &aqi),
        Line::from(format!("{}", Local::now().format("%a, %b %-d  %-I:%M %p"))).dark_gray(),
    ];
    frame.render_widget(Paragraph::new(details), detail_area);
}

/// Shown wherever the API reported no value for a reading.
const UNKNOWN: &str = "unavailable";

const DIGIT_ROWS: usize = 5;
/// Widths of the two columns in the "Now" pane, centred as a pair.
const HERO_WIDTH: u16 = 16;
const DETAIL_WIDTH: u16 = 30;

/// One 3x5 block glyph. Only digits and a minus sign are needed for temperatures.
fn glyph(c: char) -> [&'static str; DIGIT_ROWS] {
    match c {
        '0' => ["███", "█ █", "█ █", "█ █", "███"],
        '1' => ["  █", "  █", "  █", "  █", "  █"],
        '2' => ["███", "  █", "███", "█  ", "███"],
        '3' => ["███", "  █", "███", "  █", "███"],
        '4' => ["█ █", "█ █", "███", "  █", "  █"],
        '5' => ["███", "█  ", "███", "  █", "███"],
        '6' => ["███", "█  ", "███", "█ █", "███"],
        '7' => ["███", "  █", "  █", "  █", "  █"],
        '8' => ["███", "█ █", "███", "█ █", "███"],
        '9' => ["███", "█ █", "███", "  █", "███"],
        '-' => ["   ", "   ", "███", "   ", "   "],
        _ => ["   ", "   ", "   ", "   ", "   "],
    }
}

/// Renders `text` as block digits, one `String` per output row.
fn big_digits(text: &str) -> [String; DIGIT_ROWS] {
    let mut rows: [String; DIGIT_ROWS] = Default::default();

    for c in text.chars() {
        for (row, part) in rows.iter_mut().zip(glyph(c)) {
            row.push_str(part);
            row.push(' ');
        }
    }

    rows
}

fn detail_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<12}")).dark_gray(),
        Span::from(value.to_string()).white(),
    ])
}

fn forecast_area_render(frame: &mut Frame, weather: &Weather, area: Rect, unit: Unit) {
    let block = Block::bordered().title("Forecast");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [list_area, caption_area, chart_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(7),
    ])
    .areas(inner);

    // The list is the forecast only; today and the past belong to the chart.
    let upcoming = weather.daily.get(weather.today_index + 1..).unwrap_or(&[]);

    let lines: Vec<Line> = upcoming
        .iter()
        .map(|d| {
            Line::from(format!(
                "{}  {}  {:>3.0}{} / {:>3.0}{}",
                weekday(&d.date),
                emoji(d.code),
                unit.temp(d.high_c),
                unit.temp_symbol(),
                unit.temp(d.low_c),
                unit.temp_symbol(),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);

    // Widen the bars if there's room. Anything that still doesn't fit drops the
    // oldest history first, so the forecast is never what gets clipped.
    let count = weather.daily.len().max(1);
    let stride = ((chart_area.width as usize + BAR_GAP as usize) / count).clamp(2, 4);
    let capacity = ((chart_area.width as usize + BAR_GAP as usize) / stride).max(1);
    let start = weather.daily.len().saturating_sub(capacity);
    let visible = &weather.daily[start..];

    let coolest = visible
        .iter()
        .map(|d| d.high_c)
        .fold(f64::INFINITY, f64::min);
    let warmest = visible
        .iter()
        .map(|d| d.high_c)
        .fold(f64::NEG_INFINITY, f64::max);

    // Map the observed range onto BAR_FLOOR..=BAR_CEILING rather than 0..=max.
    // Scaling from zero flattens a week of similar highs into identical bars,
    // and a bar worth a few percent of the tallest rounds down to nothing.
    let span = (warmest - coolest).max(0.1);
    let scale = (BAR_CEILING - BAR_FLOOR) as f64;

    let bars: Vec<Bar> = visible
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let color = if start + i == weather.today_index {
                Color::Yellow
            } else {
                Color::Cyan
            };
            let value = BAR_FLOOR + (((d.high_c - coolest) / span) * scale).round() as u64;
            Bar::default()
                .value(value)
                .text_value(String::new())
                .style(Style::new().fg(color))
        })
        .collect();

    let caption = format!(
        "daily high · {:.0}–{:.0}{}",
        unit.temp(coolest),
        unit.temp(warmest),
        unit.temp_symbol(),
    );
    frame.render_widget(
        Paragraph::new(Line::from(caption).dark_gray()).alignment(Alignment::Center),
        caption_area,
    );

    // Centre the chart on its own measured width; left-aligned looked lopsided
    // once the bars stopped filling the pane.
    let chart_width = (visible.len() * stride).saturating_sub(BAR_GAP as usize) as u16;
    let [chart_area] = Layout::horizontal([Constraint::Length(chart_width)])
        .flex(Flex::Center)
        .areas(chart_area);

    frame.render_widget(
        BarChart::default()
            .data(BarGroup::default().bars(&bars))
            .max(BAR_CEILING)
            .bar_width(stride as u16 - BAR_GAP)
            .bar_gap(BAR_GAP),
        chart_area,
    );
}

const BAR_GAP: u16 = 1;
/// Shortest bar, as a proportion of `BAR_CEILING`. Keeps the coolest day visible.
const BAR_FLOOR: u64 = 15;
const BAR_CEILING: u64 = 100;

fn keybind_legend_render(frame: &mut Frame, app: &App, area: Rect) {
    let binds: &[(&str, &str)] = match app.screen {
        Screen::Weather => &[
            ("q", "quit"),
            ("r", "refresh"),
            ("t", "units"),
            ("l", "location"),
        ],
        Screen::Search => match &app.results {
            Fetch::Ready(l) if !l.is_empty() => {
                &[("↑↓", "navigate"), ("enter", "select"), ("esc", "cancel")]
            }
            _ => &[("enter", "search"), ("esc", "cancel")],
        },
    };

    let mut spans = vec![Span::from("  ")];
    for (key, label) in binds {
        spans.push(Span::from(format!("[{key}]")).yellow());
        spans.push(Span::from(format!(" {label}   ")).dark_gray());
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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

fn weekday(date: &str) -> String {
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(parsed) => parsed.weekday().to_string(),
        Err(_) => date.to_string(),
    }
}
