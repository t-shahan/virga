use crate::app::App;
use crate::theme::Palette;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

use super::centered;
use super::legend::{Group, reference};

/// Columns between the card's two columns when it has two.
const GUTTER: usize = 4;

/// Rows and columns the card's border and padding take.
const FRAME_ROWS: u16 = 2;
const FRAME_COLUMNS: u16 = 4;

const FOOTER: &str = "? or esc closes";

/// The full key reference, drawn over the content: every key the screen
/// answers to, under a heading for the kind of thing it does.
///
/// One binding a line, because the bar's packed rows drop bindings when the
/// terminal narrows, which is the right trade for a reminder and the wrong
/// one for the reference. The headings are what make a list this long
/// findable: someone looking for the way to another screen reads "Go to"
/// and not all of it.
///
/// The list goes in one column while the terminal is tall enough to hold
/// it, and in two when it is not, so a short terminal still shows all of
/// it. Only a terminal too short and too narrow at once clips it, from the
/// tail, where the keys the bar already names are.
pub(super) fn help_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    let groups = reference(app);
    let key_col = groups
        .iter()
        .flat_map(|group| &group.bindings)
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    let lines = laid_out(&groups, key_col, palette, area);

    let width = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(FOOTER.len()) as u16;
    let height = (lines.len() as u16)
        .saturating_add(FRAME_ROWS)
        .min(area.height);
    let card = centered(
        area,
        width.saturating_add(FRAME_COLUMNS).min(area.width),
        height,
    );

    frame.render_widget(Clear, card);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .padding(Padding::horizontal(1))
                .border_style(Style::new().fg(palette.border))
                // A block title takes the block's own style, not the
                // border's, so left alone it would render in the terminal's
                // default foreground, which is invisible on a themed ground.
                .title(Line::from("Keys").fg(palette.text))
                .title_bottom(Line::from(FOOTER).fg(palette.muted).centered()),
        ),
        card,
    );
}

/// The card's lines, in the first layout that fits the height: one
/// column, then two, then two without the blank line between groups, which
/// is what holds the longest card on a 12 row terminal. A terminal that
/// fits none of them gets the densest one it is wide enough for, clipped.
fn laid_out(groups: &[Group], key_col: usize, palette: Palette, area: Rect) -> Vec<Line<'static>> {
    let room = area.height.saturating_sub(FRAME_ROWS) as usize;
    let width = area.width as usize;
    let single = column(groups, key_col, palette, Spacing::Open);
    if single.len() <= room {
        return single;
    }
    let open = side_by_side(groups, key_col, palette, Spacing::Open, width);
    if let Some(lines) = open.filter(|lines| lines.len() <= room) {
        return lines;
    }
    side_by_side(groups, key_col, palette, Spacing::Tight, width)
        .unwrap_or_else(|| column(groups, key_col, palette, Spacing::Tight))
}

/// The groups in two columns, split where the taller column is shortest,
/// or `None` when `width` cannot hold them or there is one group.
fn side_by_side(
    groups: &[Group],
    key_col: usize,
    palette: Palette,
    spacing: Spacing,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    let height = |groups: &[Group]| column(groups, key_col, palette, spacing).len();
    let at =
        (1..groups.len()).min_by_key(|&at| height(&groups[..at]).max(height(&groups[at..])))?;
    let left = column(&groups[..at], key_col, palette, spacing);
    let right = column(&groups[at..], key_col, palette, spacing);
    let left_width = left.iter().map(Line::width).max().unwrap_or(0);
    let right_width = right.iter().map(Line::width).max().unwrap_or(0);
    if left_width + GUTTER + right_width + FRAME_COLUMNS as usize > width {
        return None;
    }

    let rows = left.len().max(right.len());
    let mut left = left.into_iter();
    let mut right = right.into_iter();
    let lines = (0..rows)
        .map(|_| {
            let mut spans = left.next().map_or_else(Vec::new, |line| line.spans);
            let used: usize = spans.iter().map(Span::width).sum();
            if let Some(line) = right.next()
                && line.width() > 0
            {
                spans.push(Span::from(" ".repeat(left_width - used + GUTTER)));
                spans.extend(line.spans);
            }
            Line::from(spans)
        })
        .collect();
    Some(lines)
}

#[derive(Clone, Copy, PartialEq)]
enum Spacing {
    /// A blank line between groups.
    Open,
    /// Groups directly under one another.
    Tight,
}

/// Groups one under another. Keys are right-aligned in a column shared by
/// the whole card, so the labels rank up whatever mix of `q` and `←→↑↓`
/// sits above them.
fn column(
    groups: &[Group],
    key_col: usize,
    palette: Palette,
    spacing: Spacing,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for group in groups {
        if spacing == Spacing::Open && !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(
            Span::from(group.heading).fg(palette.text).bold(),
        ));
        for (key, label) in &group.bindings {
            let pad = key_col.saturating_sub(key.chars().count());
            lines.push(Line::from(vec![
                Span::from(" ".repeat(pad)),
                Span::from(*key).fg(palette.selection).bold(),
                Span::from(format!("  {label}")).fg(palette.muted),
            ]));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Fetch, Screen};
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn card_on(screen: Screen, width: u16, height: u16) -> Vec<String> {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.screen = screen;
        let palette = app.theme.palette();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| help_render(frame, &app, palette, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    fn row_with<'a>(rows: &'a [String], text: &str) -> Option<&'a String> {
        rows.iter().find(|row| row.contains(text))
    }

    /// Tall enough, the card is one column with its headings in order and
    /// each key beside what it does.
    #[test]
    fn a_tall_terminal_gets_one_column_under_headings() {
        let rows = card_on(Screen::Radar, 80, 30);
        let at = |text: &str| {
            rows.iter()
                .position(|row| row.contains(text))
                .unwrap_or_else(|| panic!("{text:?} is missing from\n{}", rows.join("\n")))
        };

        assert!(at("Move") < at("Loop") && at("Loop") < at("Go to") && at("Go to") < at("App"));
        for line in [
            "←→↑↓  pan",
            "+ -  zoom",
            "n  your city",
            ", .  frame",
            "space  pause",
            "esc 1 m  forecast",
            "h 2  hourly",
            "/  search",
            "r  refresh",
            "u  units",
            "t  theme",
            "?  close this",
            "q  quit",
        ] {
            at(line);
        }
        assert!(row_with(&rows, FOOTER).is_some());
        let heading = row_with(&rows, "Move").expect("a heading");
        assert!(!heading.contains("Go to"), "one column: {heading:?}");
    }

    /// Too short for one column, the card goes to two and still shows every
    /// line, at the shortest terminal the app draws.
    #[test]
    fn a_short_terminal_gets_two_columns_and_loses_nothing() {
        for screen in [Screen::Weather, Screen::Hourly, Screen::Radar] {
            let tall = card_on(screen, 80, 40);
            let short = card_on(screen, 80, 12);
            let text = short.join("\n");

            let labels = tall
                .iter()
                .filter_map(|row| row.trim_matches(['│', ' ']).split("  ").last())
                .filter(|label| !label.is_empty() && !label.contains('─'));
            for label in labels {
                assert!(
                    text.contains(label.trim()),
                    "{screen:?} lost {label:?}:\n{text}"
                );
            }
            assert!(
                short
                    .iter()
                    .any(|row| row.contains("Move") && row.contains("  ")),
                "{text}"
            );
            assert!(text.contains(FOOTER), "{text}");
        }
    }

    #[test]
    fn the_radar_card_is_side_by_side_when_short() {
        let rows = card_on(Screen::Radar, 80, 14);
        let heading = row_with(&rows, "Move").expect("a heading");
        assert!(heading.contains("Go to"), "{heading:?}");
    }

    /// Too narrow for two columns and too short for one, the card clips
    /// from the tail and keeps its frame. Nothing panics on the way down.
    #[test]
    fn awkward_sizes_draw_without_panicking() {
        for screen in [
            Screen::Weather,
            Screen::Hourly,
            Screen::Radar,
            Screen::Search,
        ] {
            for (width, height) in [(34, 12), (20, 6), (3, 3), (1, 1), (300, 90), (34, 60)] {
                let rows = card_on(screen, width, height);
                assert_eq!(rows.len(), height as usize);
            }
        }
        let narrow = card_on(Screen::Radar, 34, 12);
        assert!(row_with(&narrow, "Move").is_some(), "{}", narrow.join("\n"));
    }
}
