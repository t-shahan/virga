//! What the next two hours of precipitation amount to, as one sentence.
//!
//! Shared by the weather screen and `virga now`, so the two cannot word the
//! same quarter hours differently. A dry window has no outlook at all: the
//! report prints nothing for an absent reading, and the screen keeps the
//! corner for the sentence it already had.

use crate::weather::model::QuarterHour;

const QUARTER_MINUTES: usize = 15;
const TWO_HOURS: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Rain,
    Snow,
}

/// The first spell of precipitation in the window. Later spells are left
/// out: one sentence has room for when the next thing changes, and a second
/// shower two hours off is the hourly screen's to show.
#[derive(Debug, PartialEq)]
pub enum Outlook {
    /// Dry now. `lasting` is absent when the spell runs past the window.
    Starting {
        kind: Kind,
        in_minutes: usize,
        lasting: Option<usize>,
    },
    Stopping {
        kind: Kind,
        in_minutes: usize,
    },
    /// Falling now and still falling when the window ends, `minutes` on.
    Throughout {
        kind: Kind,
        minutes: usize,
    },
}

/// `quarters` opens on the quarter hour in progress, which is what
/// `Weather::nowcast` holds. A quarter with no amount ends the window there:
/// what follows a hole cannot be called dry or wet.
pub fn outlook(quarters: &[QuarterHour]) -> Option<Outlook> {
    let wet: Vec<bool> = quarters
        .iter()
        .map_while(|quarter| quarter.precip_mm.map(|mm| mm > 0.0))
        .collect();
    let start = wet.iter().position(|&is_wet| is_wet)?;
    let length = wet.iter().skip(start).take_while(|&&is_wet| is_wet).count();
    let end = start + length;

    // Snow is the story when any of the spell is snow, the hourly screen's
    // rule for an hour.
    let is_snow = |quarter: &QuarterHour| quarter.snow_cm.is_some_and(|cm| cm > 0.0);
    let kind = if quarters.iter().skip(start).take(length).any(is_snow) {
        Kind::Snow
    } else {
        Kind::Rain
    };

    let ends_in_window = end < wet.len();
    Some(match (start, ends_in_window) {
        (0, true) => Outlook::Stopping {
            kind,
            in_minutes: end * QUARTER_MINUTES,
        },
        (0, false) => Outlook::Throughout {
            kind,
            minutes: wet.len() * QUARTER_MINUTES,
        },
        (_, _) => Outlook::Starting {
            kind,
            in_minutes: start * QUARTER_MINUTES,
            lasting: ends_in_window.then_some(length * QUARTER_MINUTES),
        },
    })
}

impl Outlook {
    /// "About" throughout, because the series has quarter-hour steps at
    /// best and is interpolated from hourly data over much of the world.
    pub fn sentence(&self) -> String {
        match *self {
            Self::Starting {
                kind,
                in_minutes,
                lasting: Some(lasting),
            } => format!(
                "{} in about {in_minutes} min, lasting {lasting}",
                kind.name()
            ),
            Self::Starting {
                kind,
                in_minutes,
                lasting: None,
            } => format!("{} in about {in_minutes} min", kind.name()),
            Self::Stopping { kind, in_minutes } => {
                format!("{} stopping in about {in_minutes} min", kind.name())
            }
            Self::Throughout { kind, minutes } => {
                format!("{} through the next {}", kind.name(), span(minutes))
            }
        }
    }
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Rain => "Rain",
            Self::Snow => "Snow",
        }
    }
}

/// A window trimmed by a cache read is shorter than the two hours a fresh
/// one covers, and has to say how long it actually is.
fn span(minutes: usize) -> String {
    if minutes == TWO_HOURS {
        "two hours".to_string()
    } else {
        format!("{minutes} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quarters(precip_mm: &[Option<f64>]) -> Vec<QuarterHour> {
        precip_mm
            .iter()
            .enumerate()
            .map(|(i, &precip_mm)| QuarterHour {
                time: format!("2026-08-02T{:02}:{:02}", i / 4, i % 4 * 15),
                precip_mm,
                snow_cm: Some(0.0),
            })
            .collect()
    }

    fn sentence(precip_mm: &[f64]) -> Option<String> {
        let wet: Vec<_> = precip_mm.iter().copied().map(Some).collect();
        outlook(&quarters(&wet)).map(|outlook| outlook.sentence())
    }

    #[test]
    fn a_dry_or_empty_window_has_no_outlook() {
        assert_eq!(sentence(&[0.0; 8]), None);
        assert_eq!(sentence(&[]), None);
    }

    #[test]
    fn rain_that_starts_and_ends_in_the_window_gives_both_times() {
        assert_eq!(
            sentence(&[0.0, 0.0, 0.4, 0.6, 0.2, 0.0, 0.0, 0.0]).as_deref(),
            Some("Rain in about 30 min, lasting 45")
        );
    }

    #[test]
    fn rain_that_outlasts_the_window_gives_only_its_start() {
        assert_eq!(
            sentence(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.3, 0.3]).as_deref(),
            Some("Rain in about 90 min")
        );
    }

    #[test]
    fn rain_falling_now_says_when_it_stops() {
        assert_eq!(
            sentence(&[0.1, 0.1, 0.1, 0.1, 0.1, 0.0, 0.0, 0.0]).as_deref(),
            Some("Rain stopping in about 75 min")
        );
    }

    #[test]
    fn rain_falling_throughout_names_the_window() {
        assert_eq!(
            sentence(&[0.2; 8]).as_deref(),
            Some("Rain through the next two hours")
        );
        assert_eq!(
            sentence(&[0.2; 5]).as_deref(),
            Some("Rain through the next 75 min"),
            "a trimmed window says how long it is"
        );
    }

    /// Only the first spell is worded; a second one does not stretch it.
    #[test]
    fn a_later_spell_is_left_out() {
        assert_eq!(
            sentence(&[0.3, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0]).as_deref(),
            Some("Rain stopping in about 15 min")
        );
    }

    #[test]
    fn any_snow_in_the_spell_makes_it_snow() {
        let mut window = quarters(&[Some(0.0), Some(0.4), Some(0.4), Some(0.0)]);
        window[2].snow_cm = Some(0.3);
        assert_eq!(
            outlook(&window)
                .map(|outlook| outlook.sentence())
                .as_deref(),
            Some("Snow in about 15 min, lasting 30")
        );

        // Snow in a quarter outside the spell says nothing about the spell.
        let mut window = quarters(&[Some(0.4), Some(0.0), Some(0.0)]);
        window[2].snow_cm = Some(0.3);
        assert_eq!(
            outlook(&window)
                .map(|outlook| outlook.sentence())
                .as_deref(),
            Some("Rain stopping in about 15 min")
        );
    }

    /// A quarter with no amount ends the window: rain up to a hole has not
    /// been seen to stop, and a dry run up to one says nothing of what
    /// follows.
    #[test]
    fn a_missing_amount_ends_the_window_there() {
        assert_eq!(
            outlook(&quarters(&[Some(0.2), Some(0.2), None, Some(0.0)]))
                .map(|outlook| outlook.sentence())
                .as_deref(),
            Some("Rain through the next 30 min")
        );
        assert_eq!(
            outlook(&quarters(&[Some(0.0), None, Some(0.9)])),
            None,
            "rain past a hole is not counted from now"
        );
        assert_eq!(outlook(&quarters(&[None, Some(0.9)])), None);
    }
}
