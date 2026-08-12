//! Cell-level drawing and the shared hour axis.
//!
//! Both precipitation charts write cells themselves rather than going through a
//! ratatui widget, and both hang the same hour ticks under their plot. Neither
//! is interesting on its own; keeping them here is what stops the two charts
//! from growing separate ideas of what "6a" looks like or how far a label may
//! run before it collides with its neighbour.

use crate::theme::Palette;
use chrono::{NaiveDateTime, Timelike};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

/// Rows a row of hour ticks takes.
pub(super) const TICK_ROWS: u16 = 1;

/// Columns kept clear between one tick and the next.
const TICK_GAP: u16 = 1;

/// Write one cell, clipped to the frame.
///
/// Blanks are skipped rather than written, which is only safe because ratatui
/// resets the buffer it is about to hand back. The charts scroll, so almost
/// every cell changes between frames.
pub(super) fn put(frame: &mut Frame, x: u16, y: u16, symbol: &str, colour: Color) {
    put_styled(frame, x, y, symbol, Style::new().fg(colour));
}

/// As `put`, but dimmed — a second, theme-independent step on a value ramp.
///
/// `DIM` acts on whatever foreground the palette supplied, so it separates the
/// low end of a scale in every theme without any of them defining a shade of
/// their own. Terminals that ignore SGR 2 simply render it as an ordinary cell,
/// which is why nothing is encoded by the dimming alone.
pub(super) fn put_faint(frame: &mut Frame, x: u16, y: u16, symbol: &str, colour: Color) {
    put_styled(
        frame,
        x,
        y,
        symbol,
        Style::new().fg(colour).add_modifier(Modifier::DIM),
    );
}

fn put_styled(frame: &mut Frame, x: u16, y: u16, symbol: &str, style: Style) {
    if symbol == " " {
        return;
    }
    let area = frame.area();
    if x >= area.right() || y >= area.bottom() {
        return;
    }
    frame.buffer_mut()[(x, y)]
        .set_symbol(symbol)
        .set_style(style);
}

pub(super) fn put_text(frame: &mut Frame, x: u16, y: u16, text: &str, colour: Color) {
    let mut encoded = [0u8; 4];
    for (i, ch) in text.chars().enumerate() {
        put(frame, x + i as u16, y, ch.encode_utf8(&mut encoded), colour);
    }
}

/// Right-align `text` in a gutter `width` wide ending at `x`.
pub(super) fn put_right(frame: &mut Frame, x: u16, y: u16, width: u16, text: &str, colour: Color) {
    let used = text.chars().count() as u16;
    if used > width {
        return;
    }
    put_text(frame, x + width - used, y, text, colour);
}

/// Hour ticks under a plot: every sixth hour, the weekday where the day turns
/// over, and the plot's own first column whatever hour it lands on.
///
/// The leading tick is what places the plot in the week. Without it a chart
/// whose first six-hour mark is five columns in reads as starting there, which
/// is the mistake an axis exists to prevent.
pub(super) fn hour_ticks_render(
    frame: &mut Frame,
    row: Rect,
    times: impl Iterator<Item = String>,
    stride: u16,
    palette: Palette,
) {
    // Columns already spoken for. The leading tick is wider than the rest and a
    // six-hour mark can fall right beside it, so a tick that would overprint its
    // neighbour is dropped rather than drawn on top of it.
    let mut taken = row.x;

    for (i, time) in times.enumerate() {
        let label = if i == 0 { anchor(&time) } else { tick(&time) };
        let Some(label) = label else { continue };

        let x = row.x + i as u16 * stride;
        let width = label.chars().count() as u16;
        if x < taken || x + width > row.right() {
            continue;
        }

        put_text(frame, x, row.y, &label, palette.muted);
        taken = x + width + TICK_GAP;
    }
}

fn stamp(time: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(time, "%Y-%m-%dT%H:%M").ok()
}

/// The day as well as the hour: the one tick that has to place the plot in the
/// week on its own.
fn anchor(time: &str) -> Option<String> {
    let at = stamp(time)?;
    Some(format!("{} {}", at.format("%a"), clock(at.hour())))
}

/// Every sixth hour, named by the day where it turns over — midnight is the
/// boundary the rule already marks, so naming the new day there says more than
/// "12a" would and costs nothing.
fn tick(time: &str) -> Option<String> {
    let at = stamp(time)?;
    match at.hour() {
        0 => Some(at.format("%a").to_string()),
        hour if hour % 6 == 0 => Some(clock(hour)),
        _ => None,
    }
}

/// Two or three columns, which is what a tick has between its neighbours at the
/// narrowest stride either chart draws at.
pub(super) fn clock(hour: u32) -> String {
    let suffix = if hour < 12 { 'a' } else { 'p' };
    match hour % 12 {
        0 => format!("12{suffix}"),
        hour => format!("{hour}{suffix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_read_as_a_clock_rather_than_as_a_count() {
        assert_eq!(clock(0), "12a");
        assert_eq!(clock(6), "6a");
        assert_eq!(clock(12), "12p");
        assert_eq!(clock(18), "6p");
        assert_eq!(clock(23), "11p");
    }

    /// Only every sixth hour earns a tick; midnight trades its hour for the day
    /// it starts, which is the fact a reader is actually navigating by.
    #[test]
    fn ticks_fall_every_sixth_hour_and_name_the_day_at_midnight() {
        assert_eq!(tick("2026-08-10T00:00").as_deref(), Some("Mon"));
        assert_eq!(tick("2026-08-10T06:00").as_deref(), Some("6a"));
        assert_eq!(tick("2026-08-10T12:00").as_deref(), Some("12p"));
        assert_eq!(tick("2026-08-10T07:00"), None);
        assert_eq!(tick("nonsense"), None);
    }

    #[test]
    fn the_leading_tick_carries_the_day_whatever_hour_it_lands_on() {
        assert_eq!(anchor("2026-08-10T05:00").as_deref(), Some("Mon 5a"));
        assert_eq!(anchor("2026-08-10T00:00").as_deref(), Some("Mon 12a"));
        assert_eq!(anchor("nonsense"), None);
    }
}
