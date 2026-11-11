//! What a day's high usually is at a place, for telling a warm day from an
//! unusual one.
//!
//! Three decades of daily highs are reduced, once per place, to a row per
//! day of the year. The decades themselves are not kept: 13,000 readings
//! answer no question the 366 rows do not.

use crate::weather::dto::quantile;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The baseline climatologists currently publish normals against.
const BASELINE_YEARS: std::ops::RangeInclusive<i32> = 1991..=2020;
/// Days of a leap year, so 29 February has a row of its own.
const DAYS: usize = 366;
/// Days either side of a date that count towards its row. One date over
/// thirty years is thirty readings and a ragged curve; a fortnight around it
/// is 450 and a smooth one.
const HALF_WINDOW: usize = 7;
/// Below this a row is left empty. A full window is 450 readings, so this
/// only bites where the archive has holes.
const MIN_SAMPLES: usize = 150;
/// Days the forecast and the archive must share before their difference is
/// believed.
const MIN_OVERLAP: usize = 3;
/// The share of history that counts as ordinary: the middle nineteen
/// twentieths. A high outside it is called unusual, which then happens on
/// about eighteen days a year and not on one day in ten.
const ORDINARY: (f64, f64) = (0.025, 0.975);

/// One day of the year at one place, in Celsius, from the archive's own
/// model and uncorrected.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Row {
    mean_c: f64,
    low_c: f64,
    high_c: f64,
}

impl Row {
    /// A row a person edited, or a disk damaged, can still parse. One whose
    /// bounds are out of order would call a day unusually warm and below
    /// normal at once, and one far off any thermometer would be printed.
    fn is_plausible(&self) -> bool {
        /// Wider than any surface temperature, in Celsius.
        const BOUND_C: f64 = 150.0;
        self.low_c <= self.mean_c
            && self.mean_c <= self.high_c
            && self.low_c >= -BOUND_C
            && self.high_c <= BOUND_C
    }
}

/// What a date's high usually is, corrected to the forecast's model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayNormal {
    pub mean_c: f64,
    /// The bounds of an ordinary high. Nineteen years in twenty fall inside.
    pub low_c: f64,
    pub high_c: f64,
}

/// A place's normals: a row per day of the year, and how much warmer the
/// forecast's model runs than the archive's there.
///
/// The archive is a coarser model. Over a hillside or a coast it can sit
/// degrees from the forecast every day of the year, and uncorrected that
/// would read as a town that is permanently above normal. The two are
/// compared over the days both describe and the difference is carried here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Normals {
    rows: Vec<Option<Row>>,
    bias_c: f64,
}

impl Normals {
    /// `history` is the archive's daily highs and `recent` the forecast's
    /// own for the days just gone, each keyed by date.
    pub fn from_history(history: &[(NaiveDate, f64)], recent: &[(NaiveDate, f64)]) -> Self {
        let mut by_day: Vec<Vec<f64>> = vec![Vec::new(); DAYS];
        for (date, high) in history {
            if BASELINE_YEARS.contains(&date.year())
                && let Some(readings) = by_day.get_mut(day_index(*date))
            {
                readings.push(*high);
            }
        }
        Self {
            rows: (0..DAYS).map(|day| row_for(&by_day, day)).collect(),
            bias_c: bias(history, recent),
        }
    }

    /// The normal for an ISO date such as "2026-11-11". `None` for a date
    /// that will not parse and for a day the archive did not cover.
    pub fn for_date(&self, date: &str) -> Option<DayNormal> {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        let row = (*self.rows.get(day_index(date))?)?;
        Some(DayNormal {
            mean_c: row.mean_c + self.bias_c,
            low_c: row.low_c + self.bias_c,
            high_c: row.high_c + self.bias_c,
        })
    }

    /// Whether there is anything here to compare a day against. A place the
    /// archive returned nothing for is not worth keeping on disk.
    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(Option::is_none)
    }

    /// Whether this is the shape `for_date` relies on. A table read back
    /// from disk is checked before it is trusted.
    pub fn is_well_formed(&self) -> bool {
        self.rows.len() == DAYS
            && self.bias_c.is_finite()
            && self.rows.iter().flatten().all(Row::is_plausible)
    }
}

/// A date's place in a leap year, so the same calendar day lands on the same
/// row whatever year it is in.
fn day_index(date: NaiveDate) -> usize {
    NaiveDate::from_ymd_opt(2000, date.month(), date.day())
        .map_or(0, |in_leap_year| in_leap_year.ordinal0() as usize)
}

fn row_for(by_day: &[Vec<f64>], day: usize) -> Option<Row> {
    let mut window: Vec<f64> = (0..=2 * HALF_WINDOW)
        .filter_map(|step| by_day.get((day + DAYS + step - HALF_WINDOW) % DAYS))
        .flatten()
        .copied()
        .filter(|high| high.is_finite())
        .collect();
    if window.len() < MIN_SAMPLES {
        return None;
    }
    window.sort_by(f64::total_cmp);
    Some(Row {
        mean_c: window.iter().sum::<f64>() / window.len() as f64,
        low_c: quantile(&window, ORDINARY.0)?,
        high_c: quantile(&window, ORDINARY.1)?,
    })
}

/// How much warmer the forecast's highs run than the archive's over the
/// days both describe. Zero when they share too few days to say: an
/// uncorrected normal is still a normal, and the README says it can be off.
fn bias(history: &[(NaiveDate, f64)], recent: &[(NaiveDate, f64)]) -> f64 {
    let archived: HashMap<NaiveDate, f64> = history
        .iter()
        .rev()
        .take(2 * DAYS)
        .filter(|(_, high)| high.is_finite())
        .copied()
        .collect();
    let differences: Vec<f64> = recent
        .iter()
        .filter(|(_, high)| high.is_finite())
        .filter_map(|(date, high)| Some(high - archived.get(date)?))
        .collect();
    if differences.len() < MIN_OVERLAP {
        return 0.0;
    }
    differences.iter().sum::<f64>() / differences.len() as f64
}

#[cfg(test)]
impl Normals {
    /// Every day of the year normally `mean_c`, ordinarily within `spread_c`
    /// of it, with no correction.
    pub fn flat(mean_c: f64, spread_c: f64) -> Self {
        Self {
            rows: vec![
                Some(Row {
                    mean_c,
                    low_c: mean_c - spread_c,
                    high_c: mean_c + spread_c,
                });
                DAYS
            ],
            bias_c: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
    }

    /// Every day from 1991 through 2020, its high given by `high`.
    fn baseline(high: impl Fn(NaiveDate) -> f64) -> Vec<(NaiveDate, f64)> {
        date(1991, 1, 1)
            .iter_days()
            .take_while(|day| day.year() <= 2020)
            .map(|day| (day, high(day)))
            .collect()
    }

    #[test]
    fn a_steady_climate_has_the_same_normal_every_day() {
        let normals = Normals::from_history(&baseline(|_| 12.0), &[]);

        for day in ["2026-01-01", "2026-07-15", "2024-02-29", "2026-12-31"] {
            assert_eq!(
                normals.for_date(day),
                Some(DayNormal {
                    mean_c: 12.0,
                    low_c: 12.0,
                    high_c: 12.0
                }),
                "{day}"
            );
        }
        assert!(normals.is_well_formed() && !normals.is_empty());
    }

    /// A date's row is the fortnight around it, across the turn of the year
    /// as anywhere else: 1 January draws on late December.
    #[test]
    fn a_row_pools_the_days_around_it_and_wraps_at_new_year() {
        let history = baseline(|day| if day.month() == 12 { 0.0 } else { 15.0 });
        let normals = Normals::from_history(&history, &[]);

        // 1 January: seven days of December at 0 and eight of January at 15.
        let new_year = normals.for_date("2026-01-01").expect("a row");
        assert!((new_year.mean_c - 8.0).abs() < 0.01, "{new_year:?}");
        assert_eq!((new_year.low_c, new_year.high_c), (0.0, 15.0));

        let midsummer = normals.for_date("2026-07-01").expect("a row");
        assert_eq!(midsummer.mean_c, 15.0);
    }

    /// The ordinary range is the middle of what the date has seen, so one
    /// freak week in thirty years does not stretch it.
    #[test]
    fn the_ordinary_range_ignores_the_rare_extremes() {
        let is_heatwave = |day: NaiveDate| {
            day.year() == 2003 && day.month() == 7 && (12..=18).contains(&day.day())
        };
        let history = baseline(|day| if is_heatwave(day) { 40.0 } else { 20.0 });
        let normals = Normals::from_history(&history, &[]);

        let july = normals.for_date("2026-07-15").expect("a row");
        assert_eq!((july.low_c, july.high_c), (20.0, 20.0));
        assert!(july.mean_c > 20.0 && july.mean_c < 21.0, "{july:?}");
    }

    #[test]
    fn years_outside_the_baseline_do_not_move_the_normal() {
        let mut history = baseline(|_| 10.0);
        history.extend(
            date(2021, 1, 1)
                .iter_days()
                .take(1500)
                .map(|day| (day, 30.0)),
        );
        let normals = Normals::from_history(&history, &[]);

        assert_eq!(normals.for_date("2026-06-01").map(|n| n.mean_c), Some(10.0));
    }

    /// The forecast's model ran two degrees warmer than the archive's over
    /// the week both describe, so every normal is lifted by two.
    #[test]
    fn the_normal_is_corrected_by_the_forecasts_recent_offset() {
        let mut history = baseline(|_| 10.0);
        let week: Vec<NaiveDate> = date(2026, 11, 1).iter_days().take(7).collect();
        history.extend(week.iter().map(|day| (*day, 5.0)));
        let recent: Vec<_> = week.iter().map(|day| (*day, 7.0)).collect();

        let normals = Normals::from_history(&history, &recent);

        assert_eq!(normals.for_date("2026-11-11").map(|n| n.mean_c), Some(12.0));
    }

    #[test]
    fn too_little_overlap_leaves_the_normal_uncorrected() {
        let mut history = baseline(|_| 10.0);
        history.push((date(2026, 11, 1), 5.0));
        history.push((date(2026, 11, 2), 5.0));
        let recent = [
            (date(2026, 11, 1), 9.0),
            (date(2026, 11, 2), 9.0),
            // The archive has not reached these yet.
            (date(2026, 11, 9), 9.0),
            (date(2026, 11, 10), f64::NAN),
        ];

        let normals = Normals::from_history(&history, &recent);

        assert_eq!(normals.for_date("2026-11-11").map(|n| n.mean_c), Some(10.0));
        assert!(normals.is_well_formed());
    }

    /// A short or empty archive answer is a table of gaps, never a panic,
    /// and `for_date` says nothing for a date it cannot read.
    #[test]
    fn a_thin_history_leaves_rows_empty() {
        let none = Normals::from_history(&[], &[]);
        assert!(none.is_empty() && none.is_well_formed());
        assert_eq!(none.for_date("2026-11-11"), None);

        let one_year: Vec<_> = date(1995, 1, 1)
            .iter_days()
            .take(365)
            .map(|day| (day, 10.0))
            .collect();
        assert!(Normals::from_history(&one_year, &[]).is_empty());

        let full = Normals::from_history(&baseline(|_| 10.0), &[]);
        assert_eq!(full.for_date("soon"), None);
        assert_eq!(full.for_date(""), None);
    }

    #[test]
    fn a_table_of_the_wrong_shape_is_not_well_formed() {
        let mut short = Normals::flat(10.0, 5.0);
        short.rows.truncate(12);
        assert!(!short.is_well_formed());

        let mut unbounded = Normals::flat(10.0, 5.0);
        unbounded.bias_c = f64::INFINITY;
        assert!(!unbounded.is_well_formed());
    }
}
