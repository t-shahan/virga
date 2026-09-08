//! Shared selected-forward precipitation semantics.
//!
//! A total is useful only when every hour in its requested window was
//! reported. Keeping that distinction here prevents the inspector and the
//! weathergram summary from quietly disagreeing about partial provider data.

use crate::units::Unit;
use crate::weather::model::HourlyForecast;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PrecipitationAggregate {
    Unavailable,
    Zero,
    Positive(Positive),
}

/// An amount that is definitely there. Its own type so a caller that has
/// matched it can print it without a second match, or an `expect`, on the
/// two cases that have no text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Positive {
    /// Present, but below the display precision; carries the quantum it
    /// rounds under.
    Trace(f64),
    Measured(f64),
}

/// Any run of hours, not only a slice: the week strip's days are sparse
/// arrays, and a total over them is the same claim with the same rule.
pub(super) fn aggregate<'a>(
    hours: impl IntoIterator<Item = &'a HourlyForecast>,
    unit: Unit,
) -> PrecipitationAggregate {
    let mut seen = false;
    let mut total_mm = 0.0;
    for hour in hours {
        seen = true;
        match hour.precip_mm {
            Some(amount) => total_mm += amount,
            None => return PrecipitationAggregate::Unavailable,
        }
    }
    if !seen {
        return PrecipitationAggregate::Unavailable;
    }
    classify(total_mm, unit)
}

/// The trace rule, stated once: a positive amount that rounds to zero at the
/// display precision is small, not zero, and must never print as `0.00 in`.
fn classify(total_mm: f64, unit: Unit) -> PrecipitationAggregate {
    if total_mm <= 0.0 {
        return PrecipitationAggregate::Zero;
    }

    let value = unit.precip(total_mm);
    let quantum = 0.1_f64.powi(unit.precip_decimals() as i32);
    PrecipitationAggregate::Positive(if value < quantum / 2.0 {
        Positive::Trace(quantum)
    } else {
        Positive::Measured(value)
    })
}

/// One positive reading at the unit's precision, under the same trace rule
/// as a total. Every pane that prints an amount goes through here; the four
/// copies of the quantum arithmetic this replaced agreed only by luck.
pub(super) fn measured(mm: f64, unit: Unit) -> String {
    classify(mm, unit)
        .positive_text(unit, " ")
        .unwrap_or_else(|| format!("0 {}", unit.precip_label()))
}

impl PrecipitationAggregate {
    pub(super) fn positive_text(self, unit: Unit, separator: &str) -> Option<String> {
        match self {
            Self::Positive(amount) => Some(amount.text(unit, separator)),
            Self::Unavailable | Self::Zero => None,
        }
    }
}

impl Positive {
    pub(super) fn text(self, unit: Unit, separator: &str) -> String {
        let decimals = unit.precip_decimals();
        match self {
            Self::Trace(quantum) => {
                format!("<{quantum:.decimals$}{separator}{}", unit.precip_label())
            }
            Self::Measured(value) => {
                format!("{value:.decimals$}{separator}{}", unit.precip_label())
            }
        }
    }
}
