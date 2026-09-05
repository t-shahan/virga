//! Vocabulary the bordered panes share: a labelled reading, the two titles
//! on a bottom border, a compass point, and the hour formats. Each of these
//! used to be written once per pane, and the copies were identical only
//! until one was edited.

use crate::theme::Palette;
use crate::ui::{TITLE_GUTTER, title_room};
use chrono::NaiveDateTime;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};

/// A muted label in a fixed column and the reading beside it.
pub(super) fn detail_line(label: &str, value: &str, palette: Palette) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<12}")).fg(palette.muted),
        Span::from(value.to_string()).fg(palette.text),
    ])
}

/// The period right, its summary left. The period is what changes as you
/// arrow around, so it is never the one sacrificed.
pub(super) fn bottom_titles(summary: &str, when: &str, width: u16) -> (Option<String>, String) {
    let available = title_room(width);
    let len = |s: &str| s.chars().count();

    if !summary.is_empty() && len(summary) + TITLE_GUTTER + len(when) <= available {
        return (Some(summary.to_string()), when.to_string());
    }
    (None, when.to_string())
}

/// Eight points. `rem_euclid` rather than `%`: the API has been known to
/// report a hair under zero as well as over 360.
pub(super) fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    POINTS[((degrees.rem_euclid(360.0) / 45.0).round() as usize) % POINTS.len()]
}

fn parse_hour(time: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(time, "%Y-%m-%dT%H:%M").ok()
}

/// `Mon 10 Aug, 4:00 PM`, or the raw stamp when it will not parse — a value
/// the user can still read beats a blank.
pub(super) fn long_hour(time: &str) -> String {
    parse_hour(time).map_or_else(
        || time.to_string(),
        |at| at.format("%a %-d %b, %-I:%M %p").to_string(),
    )
}

/// `4 PM`.
pub(super) fn short_hour(time: &str) -> String {
    parse_hour(time).map_or_else(|| time.to_string(), |at| at.format("%-I %p").to_string())
}

/// `Mon 10 Aug, 4 PM`. The forecast runs eight days, so every weekday name
/// occurs at least once and one occurs twice; a bare "Mon 3 AM" for
/// something a week out reads as this morning, and the date is what makes
/// it unambiguous.
pub(super) fn day_and_hour(time: &str) -> String {
    parse_hour(time).map_or_else(
        || time.to_string(),
        |at| at.format("%a %-d %b, %-I %p").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_maps_degrees_to_points() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(45.0), "NE");
        assert_eq!(compass(90.0), "E");
        assert_eq!(compass(180.0), "S");
        assert_eq!(compass(315.0), "NW");
    }

    /// 360 must not index past the end of the table, and the API has been known
    /// to report a hair over or under.
    #[test]
    fn compass_wraps_at_the_full_circle() {
        assert_eq!(compass(360.0), "N");
        assert_eq!(compass(359.0), "N");
        assert_eq!(compass(720.0), "N");
        assert_eq!(compass(-45.0), "NW");
    }

    #[test]
    fn hours_that_will_not_parse_fall_back_to_the_raw_value() {
        assert_eq!(long_hour("2026-08-10T16:00"), "Mon 10 Aug, 4:00 PM");
        assert_eq!(short_hour("2026-08-10T16:00"), "4 PM");
        assert_eq!(day_and_hour("2026-08-10T16:00"), "Mon 10 Aug, 4 PM");
        assert_eq!(long_hour("not-a-time"), "not-a-time");
        assert_eq!(short_hour(""), "");
    }

    /// The period is what changes as you arrow around, so it is never
    /// sacrificed; the summary goes first.
    #[test]
    fn bottom_keeps_the_period_and_drops_the_summary() {
        let long = "17°F above the 22-day average";

        let (summary, when) = bottom_titles(long, "Fri, Aug 21", 120);
        assert_eq!(summary.as_deref(), Some(long));
        assert_eq!(when, "Fri, Aug 21");

        let (summary, when) = bottom_titles(long, "Fri, Aug 21", 40);
        assert_eq!(summary, None);
        assert_eq!(when, "Fri, Aug 21");
    }

    #[test]
    fn bottom_omits_an_empty_summary() {
        let (summary, _) = bottom_titles("", "Today", 120);
        assert_eq!(summary, None);
    }
}
