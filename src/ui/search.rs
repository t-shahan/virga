use crate::app::{App, Fetch};
use crate::theme::Palette;
use crate::ui::{centered, spinner};
use crate::weather::model::Location;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Rows of the popup below the input: the border, the input and its
/// underline, and eight lines of results for the five the geocoder is asked
/// for.
const POPUP_HEIGHT: u16 = 12;

/// What tells apart the rows that would otherwise read the same, one entry
/// per result and `None` for a row nothing collides with.
///
/// Name, region and country are the label, and for most searches that is
/// enough. Where two rows share it, the zone tells them apart, and where the
/// zone is shared too, or unknown, the coordinates do, which two distinct
/// places cannot share. Only the colliding rows get a second line: five
/// results that never needed one would make the list twice as tall for
/// nothing.
fn disambiguators(locations: &[Location]) -> Vec<Option<String>> {
    let labels: Vec<String> = locations.iter().map(Location::label).collect();
    locations
        .iter()
        .enumerate()
        .map(|(i, location)| {
            let twins: Vec<&Location> = locations
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i && labels[*j] == labels[i])
                .map(|(_, twin)| twin)
                .collect();
            if twins.is_empty() {
                return None;
            }
            let mut parts = Vec::new();
            if let Some(zone) = &location.timezone {
                parts.push(zone.clone());
            }
            if location.timezone.is_none()
                || twins.iter().any(|twin| twin.timezone == location.timezone)
            {
                parts.push(format!("{:.2}, {:.2}", location.lat, location.lon));
            }
            Some(parts.join(" · "))
        })
        .collect()
}

pub(super) fn search_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    let second_lines = match &app.results {
        Fetch::Ready(locations) => disambiguators(locations),
        _ => Vec::new(),
    };
    // Every second line is a row the popup did not budget for, and the last
    // result must not be the one that pays: an unseen row is one the arrows
    // can still land on.
    let extra = second_lines.iter().flatten().count();
    let area = centered(
        area,
        50,
        POPUP_HEIGHT.saturating_add(u16::try_from(extra).unwrap_or(u16::MAX)),
    );
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        // Styled explicitly: a block title takes the block's own style rather
        // than the border's, so an unstyled one keeps the terminal's default
        // foreground whatever the theme says.
        .title(Line::from("Search").fg(palette.muted))
        .border_style(Style::new().fg(palette.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [input_area, list_area] = Layout::vertical([
        Constraint::Length(2), // text row + underline
        Constraint::Fill(1),
    ])
    .areas(inner);

    let input_block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(palette.muted));
    let input_inner = input_block.inner(input_area);
    frame.render_widget(input_block, input_area);

    let input_line = if app.query.is_empty() {
        Line::from(vec![
            Span::from("❯ ").fg(palette.accent),
            Span::from("search for a city").fg(palette.muted).italic(),
        ])
    } else {
        let cursor = if (app.tick / 5).is_multiple_of(2) {
            "▏"
        } else {
            " "
        };
        Line::from(vec![
            Span::from("❯ ").fg(palette.accent),
            Span::from(app.query.as_str()).fg(palette.text),
            Span::from(cursor).fg(palette.accent),
        ])
    };

    frame.render_widget(Paragraph::new(input_line), input_inner);

    let body: Vec<Line> = match &app.results {
        Fetch::Idle => Vec::new(),
        // Both of these were `.dim()` and nothing else, which left them on the
        // terminal's default foreground: on a themed ground they neither
        // repainted nor stayed reliably readable. `muted` is the role for text
        // that is meant to recede, and it carries the dimming itself — stacking
        // DIM on top of a colour already chosen to be quiet is what makes a
        // status message unreadable rather than merely secondary.
        Fetch::Loading => {
            vec![Line::from(format!("{} searching...", spinner(app.tick))).fg(palette.muted)]
        }
        Fetch::Ready(locations) if locations.is_empty() => {
            vec![Line::from("no matches").fg(palette.muted)]
        }
        Fetch::Ready(locations) => locations
            .iter()
            .zip(&second_lines)
            .enumerate()
            .flat_map(|(i, (l, second))| {
                let marker = if i == app.selected { ">" } else { " " };
                let text = format!("{marker} {}", l.label());
                let row = if i == app.selected {
                    Line::from(text).fg(palette.selection).bold()
                } else {
                    Line::from(text).fg(palette.accent)
                };
                let second = second
                    .as_ref()
                    .map(|text| Line::from(format!("    {text}")).fg(palette.muted));
                std::iter::once(row).chain(second)
            })
            .collect(),
        Fetch::Failed(e) => vec![Line::from(format!("error: {e}")).fg(palette.error)],
    };

    frame.render_widget(Paragraph::new(body), list_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Screen;
    use crate::theme::Theme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn place(name: &str, admin1: &str, timezone: Option<&str>, lat: f64, lon: f64) -> Location {
        Location {
            name: name.to_string(),
            admin1: Some(admin1.to_string()),
            country: Some("United States".to_string()),
            lat,
            lon,
            timezone: timezone.map(str::to_string),
            population: None,
        }
    }

    /// The popup drawn over a terminal of the given size, as its rows of
    /// text with the trailing blanks trimmed.
    fn drawn(locations: Vec<Location>, width: u16, height: u16) -> Vec<String> {
        let mut app = App::new();
        app.screen = Screen::Search;
        app.query = "freder".to_string();
        app.results = Fetch::Ready(locations);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let buffer = terminal
            .draw(|frame| search_render(frame, &app, Theme::default().palette(), frame.area()))
            .unwrap()
            .buffer
            .clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn rows_with(rows: &[String], needle: &str) -> Vec<String> {
        rows.iter()
            .filter(|row| row.contains(needle))
            .cloned()
            .collect()
    }

    #[test]
    fn rows_that_read_differently_get_no_second_line() {
        let found = vec![
            place(
                "Frederick",
                "Maryland",
                Some("America/New_York"),
                39.41,
                -77.41,
            ),
            place(
                "Frederick",
                "Colorado",
                Some("America/Denver"),
                40.10,
                -104.94,
            ),
        ];

        assert_eq!(disambiguators(&found), [None, None]);

        let rows = drawn(found, 60, 20);
        assert!(rows_with(&rows, "America/").is_empty(), "{rows:#?}");
    }

    /// Two rows with one label: the zone separates them, so that is what the
    /// second line carries, and nothing else.
    #[test]
    fn rows_that_read_the_same_are_told_apart_by_their_zone() {
        let found = vec![
            place(
                "Frederick",
                "Maryland",
                Some("America/New_York"),
                39.41,
                -77.41,
            ),
            place(
                "Frederick",
                "Maryland",
                Some("America/Chicago"),
                38.90,
                -76.90,
            ),
            place(
                "Frederick",
                "Colorado",
                Some("America/Denver"),
                40.10,
                -104.94,
            ),
        ];

        assert_eq!(
            disambiguators(&found),
            [
                Some("America/New_York".to_string()),
                Some("America/Chicago".to_string()),
                None
            ]
        );
    }

    /// The same zone tells nothing apart, and neither does no zone; the
    /// coordinates step in, since two places cannot share those.
    #[test]
    fn a_shared_or_unknown_zone_falls_back_to_the_coordinates() {
        let found = vec![
            place(
                "Frederick",
                "Maryland",
                Some("America/New_York"),
                39.41,
                -77.41,
            ),
            place(
                "Frederick",
                "Maryland",
                Some("America/New_York"),
                38.90,
                -76.90,
            ),
            place("Frederick", "Maryland", None, 39.00, -77.00),
        ];

        assert_eq!(
            disambiguators(&found),
            [
                Some("America/New_York · 39.41, -77.41".to_string()),
                Some("America/New_York · 38.90, -76.90".to_string()),
                Some("39.00, -77.00".to_string())
            ]
        );
    }

    /// The second line sits under its row, indented and in the muted
    /// colour, and the rows below it move down rather than vanish.
    #[test]
    fn the_second_line_is_drawn_under_its_row() {
        let found = vec![
            place(
                "Frederick",
                "Maryland",
                Some("America/New_York"),
                39.41,
                -77.41,
            ),
            place(
                "Frederick",
                "Maryland",
                Some("America/Chicago"),
                38.90,
                -76.90,
            ),
            place(
                "Frederick",
                "Colorado",
                Some("America/Denver"),
                40.10,
                -104.94,
            ),
        ];

        let rows = drawn(found, 60, 20);
        let inside: Vec<&str> = rows
            .iter()
            .filter(|row| row.contains("Frederick") || row.contains("America/"))
            .map(|row| row.trim_matches(|c| c == ' ' || c == '│').trim_end())
            .collect();

        assert_eq!(
            inside,
            [
                "> Frederick, Maryland, United States",
                "America/New_York",
                "Frederick, Maryland, United States",
                "America/Chicago",
                "Frederick, Colorado, United States",
            ],
            "{rows:#?}"
        );
        let second = rows
            .iter()
            .find(|row| row.contains("America/New_York"))
            .unwrap();
        assert!(second.contains("│    America/"), "not indented: {second:?}");
    }

    /// The narrowest terminal the app draws in: the label is cut at the
    /// border, and the second line, being shorter, is what still tells the
    /// two rows apart.
    #[test]
    fn the_second_line_survives_the_narrowest_terminal() {
        let found = vec![
            place(
                "Frederick",
                "Maryland",
                Some("America/New_York"),
                39.41,
                -77.41,
            ),
            place(
                "Frederick",
                "Maryland",
                Some("America/Chicago"),
                38.90,
                -76.90,
            ),
        ];

        let rows = drawn(found, 34, 12);

        assert_eq!(rows_with(&rows, "America/New_York").len(), 1, "{rows:#?}");
        assert_eq!(rows_with(&rows, "America/Chicago").len(), 1, "{rows:#?}");
        assert!(
            rows.iter().all(|row| row.chars().count() <= 34),
            "a row ran past the terminal: {rows:#?}"
        );
    }

    /// Five colliding rows want ten lines of an eight-line list. The popup
    /// grows by the lines it added rather than losing the last result
    /// behind its own border.
    #[test]
    fn the_popup_grows_by_the_second_lines_it_added() {
        let found: Vec<Location> = (0..5)
            .map(|i| {
                place(
                    "Frederick",
                    "Maryland",
                    Some(&format!("Zone/{i}")),
                    39.0 + f64::from(i),
                    -77.0,
                )
            })
            .collect();

        let rows = drawn(found, 60, 24);
        let border_rows = rows.iter().filter(|row| row.contains('─')).count();

        assert_eq!(rows_with(&rows, "Frederick").len(), 5, "{rows:#?}");
        assert_eq!(rows_with(&rows, "Zone/4").len(), 1, "{rows:#?}");
        // Top border, the input's underline, and the bottom border: the
        // popup is 17 rows tall, which is the 12 it was plus one per line.
        assert_eq!(border_rows, 3, "{rows:#?}");
        let top = rows.iter().position(|row| row.contains('─')).unwrap();
        let bottom = rows.iter().rposition(|row| row.contains('─')).unwrap();
        assert_eq!(bottom - top + 1, 17, "{rows:#?}");
    }

    #[test]
    fn a_list_with_no_collisions_keeps_the_popup_its_usual_height() {
        let found = vec![
            place(
                "Frederick",
                "Maryland",
                Some("America/New_York"),
                39.41,
                -77.41,
            ),
            place(
                "Frederick",
                "Colorado",
                Some("America/Denver"),
                40.10,
                -104.94,
            ),
        ];

        let rows = drawn(found, 60, 24);
        let top = rows.iter().position(|row| row.contains('─')).unwrap();
        let bottom = rows.iter().rposition(|row| row.contains('─')).unwrap();

        assert_eq!(bottom - top + 1, usize::from(POPUP_HEIGHT), "{rows:#?}");
    }
}
