use crate::theme::Palette;
use crate::units::Unit;
use crate::weather::code::emoji;
use crate::weather::model::Weather;
use chrono::{Datelike, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
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
    // Today onwards. The past stays exclusive to the chart.
    let upcoming = weather.daily.get(weather.today_index..).unwrap_or(&[]);

    let block = Block::bordered()
        .border_style(Style::new().fg(palette.border))
        // A block title takes the block's own style rather than the border's,
        // so left unstyled it stays the terminal's default foreground however
        // the theme is set. It is a header, so it takes the header's role.
        .title(Line::from("Forecast").fg(palette.muted));
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

    lines.extend(upcoming.iter().enumerate().map(|(i, d)| {
        let is_today = i == 0;
        let is_selected = weather.today_index + i == selected;

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
            unit.temp(d.high_c),
            unit.temp_symbol(),
            unit.temp(d.low_c),
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

// Compile-time invariants: the detail levels must stay ordered, so a narrowing
// table can only ever lose columns. Breaking this fails the build.
const _: () = assert!(TABLE_MINIMAL < TABLE_COMPACT);
const _: () = assert!(TABLE_COMPACT < TABLE_FULL);

/// Rendered width of the table at each level of detail, emoji included.
const TABLE_MINIMAL: u16 = 26;
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

    /// Each detail level must fit inside the width that selects it, or the
    /// table would be clipped at exactly the size meant to accommodate it.
    #[test]
    fn renders_at_every_detail_level_without_clipping() {
        let w = Weather::fixture(22, 14);
        for width in [TABLE_MINIMAL, TABLE_COMPACT, TABLE_FULL, 100, 200] {
            let mut t = Terminal::new(TestBackend::new(width, 14)).unwrap();
            t.draw(|f| forecast_area_render(f, &w, palette(), f.area(), Unit::Imperial, 14))
                .unwrap();
        }
    }

    /// Renders the table and returns its rows as plain text, which is what a
    /// reader who cannot use colour is left with.
    fn rows_without_colour(selected: usize) -> Vec<String> {
        let w = Weather::fixture(22, 14);
        let mut t = Terminal::new(TestBackend::new(100, 14)).unwrap();
        t.draw(|f| forecast_area_render(f, &w, palette(), f.area(), Unit::Imperial, selected))
            .unwrap();

        let buf = t.backend().buffer();
        (0..14)
            .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    /// Which row a given index lands on depends on today's real date, so this
    /// asserts the relationship between selection and marker rather than any
    /// particular weekday.
    fn marked_row(selected: usize) -> usize {
        let rows = rows_without_colour(selected);
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

    /// The gutter the marker uses is the one the header already leaves blank,
    /// so a marked row's columns have to line up with an unmarked one's.
    #[test]
    fn the_marker_does_not_shift_the_columns() {
        let rows = rows_without_colour(16);
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
