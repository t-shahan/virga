use crate::theme::Palette;
use crate::units::Unit;
use crate::weather::code::emoji;
use crate::weather::model::Weather;
use chrono::{Datelike, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

pub(super) fn forecast_area_render(
    frame: &mut Frame,
    weather: &Weather,
    palette: Palette,
    area: Rect,
    unit: Unit,
    selected: usize,
) {
    // Today onwards, until the selection steps into the past — then the
    // window slides back just far enough to keep the selected day on the
    // list, dropping days off the far end instead of growing. The chart
    // below always draws the whole range; before the slide, selecting a
    // past day highlighted a bar whose details appeared nowhere.
    let start = weather.today_index.min(selected);
    let window_len = weather.daily.len().saturating_sub(weather.today_index);
    let shown = weather
        .daily
        .get(start..weather.daily.len().min(start + window_len))
        .unwrap_or(&[]);

    let block = framed(palette);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let table_area = inner;

    // Columns are dropped from the right as the table narrows, so a resize
    // costs detail rather than breaking the alignment. Decided before the area
    // is centred, or the narrowing would feed back into the column choice.
    let show_conditions = table_area.width >= TABLE_COMPACT;
    let show_all = table_area.width >= TABLE_FULL;

    // Centre the block rather than the lines: Alignment::Center would centre
    // each row on its own width, and the emoji ending every row varies in cell
    // width, so the columns would wobble line to line. Side by side this is a
    // no-op, the column already being exactly the table's width.
    let block_width = if show_all {
        TABLE_FULL
    } else if show_conditions {
        TABLE_COMPACT
    } else {
        TABLE_MINIMAL
    };
    let [table_area] = Layout::horizontal([Constraint::Length(block_width.min(table_area.width))])
        .flex(Flex::Center)
        .areas(table_area);

    // Emoji cell widths vary between glyphs, so the icon sits at the end of the
    // row where it cannot push the numeric columns out of alignment.
    let mut header = format!("  {:<5}{:>7}{:>8}", "day", "high", "low");
    if show_conditions {
        header += &format!("{:>7}{:>8}", "rain", "wind");
    }
    if show_all {
        header += &format!("{:>7}{:>10}{:>9}", "uv", "sunrise", "sunset");
    }

    let mut lines = vec![Line::from(header).fg(palette.muted)];

    lines.extend(shown.iter().enumerate().map(|(i, d)| {
        let is_today = start + i == weather.today_index;
        let is_selected = start + i == selected;

        let day = if is_today {
            "Today".to_string()
        } else {
            weekday(&d.date)
        };

        // A caret in the gutter the header already leaves empty, so the
        // selection survives being unable to tell yellow from light blue —
        // colour-blindness, a monochrome terminal, or a screenshot. Same
        // marker the search screen uses, and ASCII on purpose: a geometric
        // arrow is ambiguous-width in some terminals and would shift every
        // column on the selected row by a cell.
        let marker = if is_selected { '>' } else { ' ' };

        // The high/low widths in the heading are these value widths plus the
        // two-cell unit symbol, so the headings sit over their own columns.
        let mut row = format!(
            "{marker} {:<5}{:>5.0}{}{:>6.0}{}",
            day,
            unit.temp_rounded(d.high_c),
            unit.temp_symbol(),
            unit.temp_rounded(d.low_c),
            unit.temp_symbol(),
        );

        if show_conditions {
            let rain = d
                .rain_chance
                .map_or_else(|| DASH.to_string(), |p| format!("{p}%"));
            let wind = d.wind_kph.map_or_else(
                || DASH.to_string(),
                |kph| format!("{:.0} {}", unit.speed(kph), unit.speed_label()),
            );
            row += &format!("{rain:>7}{wind:>8}");
        }

        if show_all {
            let uv = d
                .uv_index
                .map_or_else(|| DASH.to_string(), |uv| format!("{uv:.0}"));
            let sunrise = d.sunrise.as_deref().map_or(DASH, clock);
            let sunset = d.sunset.as_deref().map_or(DASH, clock);
            row += &format!("{uv:>7}{sunrise:>10}{sunset:>9}");
        }

        row += &format!("   {}", emoji(d.code));

        // Matches the chart: yellow marks the selection, today keeps a quieter
        // tint so it stays findable once the selection has moved off it.
        let line = Line::from(row);
        if is_selected {
            line.fg(palette.selection).bold()
        } else if is_today {
            line.fg(palette.now)
        } else {
            line.fg(palette.text)
        }
    }));

    frame.render_widget(Paragraph::new(lines), table_area);
}

/// The pane's border and heading: the part of it that does not wait for
/// weather.
fn framed(palette: Palette) -> Block<'static> {
    Block::bordered()
        .border_style(Style::new().fg(palette.border))
        // A block title takes the block's own style rather than the border's,
        // so left unstyled it stays the terminal's default foreground however
        // the theme is set. It is a header, so it takes the header's role.
        .title(Line::from("Forecast").fg(palette.muted))
}

/// The pane on a cache miss: its border and heading, with `status` — the
/// spinner and what it is waiting on — centred where the rows will be. The
/// spinner used to float in a popup over an empty terminal; here it sits in
/// the box the forecast will fill, so the frame reads as a screen that is
/// waiting rather than as one that has not been drawn.
pub(super) fn forecast_skeleton_render(
    frame: &mut Frame,
    palette: Palette,
    area: Rect,
    status: &str,
) {
    let block = framed(palette);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [row] = Layout::vertical([Constraint::Length(1)])
        .flex(Flex::Center)
        .areas(inner);
    frame.render_widget(
        Paragraph::new(status)
            .style(Style::new().fg(palette.text))
            .alignment(Alignment::Center),
        row,
    );
}

// Compile-time invariants: the detail levels must stay ordered, so a narrowing
// table can only ever lose columns. Breaking this fails the build.
const _: () = assert!(TABLE_MINIMAL < TABLE_COMPACT);
const _: () = assert!(TABLE_COMPACT < TABLE_FULL);

/// Rendered width of the table at each level of detail, emoji included.
/// Each is the row's exact cell count, and the count has to be exact: the
/// emoji is a wide grapheme at the row's end, and a tier one cell short
/// does not clip it but drops it whole, silently, at every width the tier
/// covers. The minimal row is 22 cells of text, three of padding and the
/// two-cell emoji.
pub(super) const TABLE_MINIMAL: u16 = 27;
const TABLE_COMPACT: u16 = 42;
pub(super) const TABLE_FULL: u16 = 68;

const DASH: &str = "–";

/// "2026-08-09T06:17" -> "06:17". Falls back to the raw value if the shape
/// is not what we expect, rather than guessing.
fn clock(stamp: &str) -> &str {
    stamp.get(11..16).unwrap_or(stamp)
}

fn weekday(date: &str) -> String {
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(parsed) => parsed.weekday().to_string(),
        Err(_) => date.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn palette() -> Palette {
        Theme::default().palette()
    }
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn weekday_falls_back_to_the_raw_value() {
        assert_eq!(weekday("2026-08-11"), "Tue");
        assert_eq!(weekday("nonsense"), "nonsense");
    }

    #[test]
    fn clock_slices_the_time_out_of_a_timestamp() {
        assert_eq!(clock("2026-08-09T06:17"), "06:17");
        assert_eq!(clock("short"), "short");
    }

    /// Renders the table into a terminal `width` cells wide and returns its
    /// rows as plain text, which is what a reader who cannot use colour is
    /// left with. Row 0 is the border; the header is row 1.
    fn rendered_rows(width: u16, selected: usize) -> Vec<String> {
        let w = Weather::fixture(22, 14);
        let mut t = Terminal::new(TestBackend::new(width, 14)).unwrap();
        t.draw(|f| forecast_area_render(f, &w, palette(), f.area(), Unit::Imperial, selected))
            .unwrap();

        let buf = t.backend().buffer();
        (0..14)
            .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    /// The border takes a cell each side, so a tier is exercised at exactly
    /// the inner width that selects it by asking for two more.
    fn rows_at_inner_width(inner: u16) -> Vec<String> {
        rendered_rows(inner + 2, 14)
    }

    /// The data rows: every one carries a temperature, and nothing else does.
    fn data_rows(rows: &[String]) -> impl Iterator<Item = &String> {
        rows.iter().filter(|row| row.contains('°'))
    }

    /// Widths past every tier only add margin: the full tier's last column
    /// and the emoji are still there.
    #[test]
    fn renders_at_generous_widths() {
        for width in [100, 200] {
            let rows = rendered_rows(width, 14);
            assert!(rows[1].contains("sunset"), "{:?}", rows[1]);
            for row in data_rows(&rows) {
                assert!(row.contains(emoji(0)), "width {width}: {row:?}");
            }
        }
    }

    /// Every tier constant counts the emoji as two cells, and a glyph that
    /// measured wider would be dropped whole again with every other test
    /// green, since the fixture only ever shows a clear sky. So every code
    /// the table can receive, known or not, is rendered and measured.
    #[test]
    fn every_condition_emoji_is_two_cells_wide() {
        for code in 0..=u8::MAX {
            let glyph = emoji(code);
            let mut t = Terminal::new(TestBackend::new(6, 1)).unwrap();
            t.draw(|f| f.render_widget(Paragraph::new(format!("{glyph}x")), f.area()))
                .unwrap();
            assert_eq!(
                t.backend().buffer()[(2, 0)].symbol(),
                "x",
                "code {code}: {glyph:?} is not two cells wide"
            );
        }
    }

    /// The issue: the minimal tier was declared one cell narrower than its
    /// rows, so the emoji ending every row was dropped at every width the
    /// tier covers and first appeared at the compact tier. Each tier is
    /// checked at exactly its own width for every column it promises and
    /// for the emoji on every data row, not merely for surviving the render.
    #[test]
    fn every_detail_level_shows_every_column_it_promises_at_its_own_width() {
        let clear_sky = emoji(0);
        for (inner, promised) in [
            (TABLE_MINIMAL, &["day", "high", "low"][..]),
            (TABLE_COMPACT, &["day", "high", "low", "rain", "wind"][..]),
            (
                TABLE_FULL,
                &[
                    "day", "high", "low", "rain", "wind", "uv", "sunrise", "sunset",
                ][..],
            ),
        ] {
            let rows = rows_at_inner_width(inner);
            let header = &rows[1];
            for column in promised {
                assert!(
                    header.contains(column),
                    "no {column} column at inner width {inner}: {header:?}"
                );
            }
            assert!(data_rows(&rows).count() >= 8, "{rows:#?}");
            for row in data_rows(&rows) {
                assert!(
                    row.contains(clear_sky),
                    "the condition emoji is dropped at inner width {inner}: {row:?}"
                );
            }
        }
    }

    /// One cell below a tier the table falls to the tier beneath, whole,
    /// rather than clipping the wider one.
    #[test]
    fn one_cell_short_of_a_tier_falls_back_to_the_tier_beneath() {
        for (inner, lost, kept) in [
            (TABLE_COMPACT - 1, "rain", "low"),
            (TABLE_FULL - 1, "uv", "rain"),
        ] {
            let rows = rows_at_inner_width(inner);
            assert!(!rows[1].contains(lost), "{:?}", rows[1]);
            assert!(rows[1].contains(kept), "{:?}", rows[1]);
            for row in data_rows(&rows) {
                assert!(row.contains(emoji(0)), "inner width {inner}: {row:?}");
            }
        }
    }

    /// The marker's row follows from the fixture's fixed dates and today
    /// index, but the tests below assert the relationship between selection
    /// and marker rather than a particular row, so a fixture change cannot
    /// break them.
    fn marked_row(selected: usize) -> usize {
        let rows = rendered_rows(100, selected);
        let marked: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.contains('>'))
            .map(|(y, _)| y)
            .collect();

        assert_eq!(
            marked.len(),
            1,
            "exactly one row should carry the marker, got {marked:?}"
        );
        marked[0]
    }

    /// The finding: with the selection carried by yellow-versus-light-blue,
    /// stripping the colour left nothing at all to identify the selected row.
    #[test]
    fn the_selection_survives_losing_colour() {
        // today_index is 14 in the fixture; the assertion is only that a
        // marker exists and is unique, which `marked_row` checks.
        marked_row(14);
        marked_row(16);
    }

    #[test]
    fn the_marker_travels_with_the_selection() {
        let today = marked_row(14);

        for step in 1..=4 {
            assert_eq!(
                marked_row(14 + step),
                today + step,
                "the marker did not follow the selection {step} day(s) on"
            );
        }
    }

    /// The issue: stepping the selection into the past highlighted a bar in
    /// the chart whose details appeared nowhere — the table stayed pinned to
    /// today onwards. The window now slides back to keep the selection listed.
    #[test]
    fn the_window_slides_back_to_a_selected_past_day() {
        // Two days into the past: the selected day leads the list — the same
        // row today leads it from — and Today follows two rows later, still
        // labelled as such.
        let top = marked_row(14);
        assert_eq!(
            marked_row(12),
            top,
            "the selected past day should lead the list"
        );

        let rows = rendered_rows(100, 12);
        let today = rows
            .iter()
            .position(|r| r.contains("Today"))
            .expect("Today stays listed");
        assert_eq!(
            today,
            top + 2,
            "Today should sit two rows below the selection"
        );
    }

    /// Sliding back must not grow the table: a day gained off the back costs
    /// one off the far end, so the height the layout reserved stays honest.
    #[test]
    fn the_window_keeps_its_size_when_it_slides() {
        let rows_at = |selected: usize| {
            rendered_rows(100, selected)
                .iter()
                .filter(|r| !r.trim().is_empty())
                .count()
        };

        assert_eq!(rows_at(10), rows_at(14));
        assert_eq!(rows_at(0), rows_at(14), "even from the oldest day");
    }

    /// The gutter the marker uses is the one the header already leaves blank,
    /// so a marked row's columns have to line up with an unmarked one's.
    #[test]
    fn the_marker_does_not_shift_the_columns() {
        let rows = rendered_rows(100, 16);
        let marked = rows.iter().find(|r| r.contains('>')).expect("a marked row");
        let plain = rows
            .iter()
            .find(|r| r.contains("Fri"))
            .expect("an unmarked row");

        let degree = |row: &str| row.find('°').expect("a temperature column");
        assert_eq!(
            degree(marked),
            degree(plain),
            "marked {marked:?} and unmarked {plain:?} disagree on where the column starts"
        );
    }
}
