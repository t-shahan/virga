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
    if total_mm <= 0.0 {
        return PrecipitationAggregate::Zero;
    }

    let value = unit.precip(total_mm);
    let quantum = 0.1_f64.powi(unit.precip_decimals() as i32);
    if value < quantum / 2.0 {
        PrecipitationAggregate::Trace(quantum)
    } else {
        PrecipitationAggregate::Measured(value)
    }
}

impl PrecipitationAggregate {
    pub(super) fn positive_text(self, unit: Unit, separator: &str) -> Option<String> {
        let decimals = unit.precip_decimals();
        match self {
            Self::Trace(quantum) => Some(format!(
                "<{quantum:.decimals$}{separator}{}",
                unit.precip_label()
            )),
            Self::Measured(value) => Some(format!(
                "{value:.decimals$}{separator}{}",
                unit.precip_label()
            )),
            Self::Unavailable | Self::Zero => None,
        }
    }
}
