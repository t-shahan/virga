//! The classic hourly screen behind `v`: the selected-hour inspector above
//! the mirrored precipitation chart, chance rising and amount hanging below.
//! The inspector's frame, titles and precipitation summaries are shared with
//! the weathergram screen in `inspector.rs`; this file owns what is
//! particular to the classic view — a chance hero, the readings beside it,
//! and how the panels stack.

use crate::app::App;
use crate::theme::Palette;
use crate::ui::inspector::{self, Hero, amount_line, inspector_render, total_line, window_from};
use crate::ui::pane::{detail_line, short_hour};
use crate::ui::precip_chart::precip_chart_render;
use crate::ui::precip_week::precip_week_render;
use crate::ui::{UNKNOWN, precip_chart, precip_week};
use crate::units::Unit;
use crate::weather::model::HourlyForecast;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;

pub(super) fn classic_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    let crate::app::Fetch::Ready(weather) = &app.weather else {
        return;
    };

    let hours = weather.forecast_hours();
    let selected = app.selected_hour;
    let hour = hours.get(selected);

    // Three readings of the same forecast, coarsening downward: this hour, the
    // day or so either side of it, then the week. Each box answers a question
    // the one above it cannot, which is what earns the rows — the alternative
    // tried first was to spend surplus height on more of the hourly chart, and
    // three bands of the same bars read as repetition rather than as depth.
    //
    // The chart's box is sized to its plot rather than filled. A plot centred
    // in a taller box leaves blank rows below the bars as well as above, and
    // the ones below are the pair an axis cannot explain away.
    // Slack falls below the last box rather than inside any of them. Blank rows
    // in a bordered box read as a chart that failed; the same rows under the
    // last one read as the margin they are.
    //
    // The strip is measured against the chart's *comfortable* height, not its
    // maximum. The chart may grow past this when nothing else wants the rows,
    // but it does not get to crowd the strip out first.
    let week = inspector::week_strip(hours, area, inspector::ROWS + precip_chart::COMFORT_ROWS);
    let [pane, chart, week_area, _margin] = Layout::vertical([
        Constraint::Length(inspector::ROWS),
        Constraint::Max(precip_chart::BOX_ROWS),
        Constraint::Length(
            week.as_ref()
                .map_or(0, |(_, rows)| precip_week::box_rows(*rows)),
        ),
        Constraint::Fill(1),
    ])
    .areas(area);

    inspector_render(
        frame,
        app,
        hours,
        palette,
        pane,
        &hero_chance(hour),
        detail_lines(hours, selected, palette, app.unit),
    );
    precip_chart_render(frame, weather, palette, chart, app.unit, selected);
    if let Some((days, _)) = &week {
        precip_week_render(frame, days, palette, week_area, app.unit, selected);
    }
}

/// Chance is the hero because it is defined for every hour and it is what the
/// rising half of the chart encodes. Amount would read `0` for most of the
/// week. It appears here and nowhere else on the screen.
fn hero_chance(hour: Option<&HourlyForecast>) -> Hero<'static> {
    let value = hour
        .and_then(|h| h.chance)
        .map_or_else(|| "--".to_string(), |chance| chance.to_string());
    Hero { value, suffix: "%" }
}

/// This hour, then the day ahead of it. Chance is deliberately absent — it is
/// the hero, and saying it twice more was the first draft's mistake.
fn detail_lines(
    hours: &[HourlyForecast],
    selected: usize,
    palette: Palette,
    unit: Unit,
) -> Vec<Line<'static>> {
    let hour = hours.get(selected);
    let ahead = window_from(hours, selected);

    let temperature = hour.and_then(|h| h.temp_c).map_or_else(
        || UNKNOWN.to_string(),
        |c| format!("{:.0}{}", unit.temp_rounded(c), unit.temp_symbol()),
    );

    vec![
        detail_line("amount", &amount_line(hour, unit), palette),
        detail_line("temperature", &temperature, palette),
        detail_line("24 h total", &total_line(ahead, unit), palette),
        detail_line("24 h peak", &peak_line(ahead), palette),
        detail_line("wet hours", &wet_hours_line(ahead), palette),
    ]
}

fn peak_line(ahead: &[HourlyForecast]) -> String {
    let peak = ahead
        .iter()
        .filter_map(|h| Some((h.chance?, h.time.as_str())))
        .max_by_key(|(chance, _)| *chance);

    match peak {
        Some((chance, time)) => format!("{chance}% at {}", short_hour(time)),
        None => UNKNOWN.to_string(),
    }
}

/// A count needs every hour to have answered. An unreported hour counted as
/// dry would make "0 of 24" the reading for a window the provider dropped.
fn wet_hours_line(ahead: &[HourlyForecast]) -> String {
    if ahead.is_empty() || ahead.iter().any(|h| h.precip_mm.is_none()) {
        return UNKNOWN.to_string();
    }
    format!(
        "{} of {}",
        ahead.iter().filter(|h| h.is_wet()).count(),
        ahead.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActiveLocation, Fetch, Screen};
    use crate::theme::Theme;
    use crate::ui::inspector::{DETAIL_WIDTH, dry_hours, evening_hours};
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn palette() -> Palette {
        Theme::default().palette()
    }

    const CITY: &str = "Frederick, Maryland, United States";

    fn app_showing(hours: Vec<HourlyForecast>, selected: usize) -> App {
        let mut weather = Weather::fixture(22, 14);
        weather.hourly = hours;
        weather.now_hour = 0;

        let mut app = App::new();
        app.screen = Screen::Hourly;
        app.location = ActiveLocation {
            label: CITY.to_string(),
            ..Default::default()
        };
        app.weather = Fetch::Ready(weather);
        app.selected_hour = selected;
        app
    }

    /// The rows the strip draws are calendar dates, so the rows the layout
    /// reserves have to be counted the same way. The weathergram screen has
    /// the same test; the strip is sized by a different budget here, so it
    /// is proved again.
    #[test]
    fn the_week_strip_keeps_a_last_partial_day_it_has_room_for() {
        // 60 hours from 6 PM: six hours today, two whole days, six hours of a
        // fourth date. Hour 57 is on that fourth date.
        let app = app_showing(evening_hours(60), 57);
        let text = rendered(100, 29, &app);

        let rows: Vec<&str> = text
            .lines()
            .skip_while(|line| !line.contains("this week"))
            // The title border, then the hour axis.
            .skip(2)
            .take_while(|line| !line.contains('└'))
            .collect();

        assert_eq!(
            rows.len(),
            4,
            "60 hours from 6 PM is four dates, not {}:\n{text}",
            rows.len()
        );
        assert!(rows[0].contains("Today"), "{:?}", rows[0]);
        assert!(
            rows[3].contains('▸'),
            "the selection is on the fourth date but no row is marked:\n{text}"
        );
    }

    #[test]
    fn the_summary_rows_look_a_day_ahead_from_the_selection() {
        let mut hours = dry_hours(96);
        for hour in hours.iter_mut().take(30).skip(26) {
            hour.precip_mm = Some(1.0);
        }

        // From hour 10, the window 10..34 catches all four wet hours.
        let ahead = window_from(&hours, 10);
        assert_eq!(ahead.len(), 24);
        assert_eq!(wet_hours_line(ahead), "4 of 24");

        // From hour 0, the window 0..24 catches none of them.
        assert_eq!(wet_hours_line(window_from(&hours, 0)), "0 of 24");
    }

    /// The window must shorten rather than run off the end of the series,
    /// and the count says how many hours it actually covers.
    #[test]
    fn the_wet_hour_count_names_a_clipped_window() {
        let hours = dry_hours(30);
        assert_eq!(wet_hours_line(window_from(&hours, 20)), "0 of 10");
        assert_eq!(wet_hours_line(window_from(&hours, 30)), UNKNOWN);
    }

    #[test]
    fn a_wet_hour_count_needs_every_hour_reported() {
        let mut hours = dry_hours(24);
        hours[7].precip_mm = None;
        assert_eq!(wet_hours_line(&hours), UNKNOWN);
    }

    #[test]
    fn the_peak_names_the_hour_it_falls_in() {
        let mut hours = dry_hours(48);
        hours[15].chance = Some(80);

        assert_eq!(peak_line(window_from(&hours, 0)), "80% at 3 PM");
    }

    /// The bug this screen had and the weathergram's inspector did not: a
    /// window the provider dropped summed to zero and was reported as a dry
    /// day, with "0 of 24" wet hours to back it up.
    #[test]
    fn full_total_is_unavailable_when_every_measurement_is_missing() {
        let mut hours = dry_hours(24);
        for hour in &mut hours {
            hour.precip_mm = None;
        }

        let app = app_showing(hours, 0);
        let text = rendered(100, 16, &app);
        for label in ["24 h total", "wet hours"] {
            let line = text
                .lines()
                .find(|line| line.contains(label))
                .unwrap_or_else(|| panic!("no {label} line:\n{text}"));
            assert!(
                line.contains(UNKNOWN),
                "{label} claimed dryness from missing data: {line:?}"
            );
        }
    }

    /// One hole is enough. The all-missing render above proves the wording
    /// reaches the screen; this proves a single unreported hour, in a window
    /// that would otherwise sum to something, is not summed around.
    #[test]
    fn a_single_unreported_hour_renders_neither_a_total_nor_a_count() {
        let mut hours = dry_hours(24);
        hours[2].precip_mm = Some(3.0);
        hours[7].precip_mm = None;

        let app = app_showing(hours, 0);
        for width in [60, 100] {
            let text = rendered(width, 16, &app);
            for label in ["24 h total", "wet hours"] {
                let line = text
                    .lines()
                    .find(|line| line.contains(label))
                    .unwrap_or_else(|| panic!("no {label} line at {width}:\n{text}"));
                assert!(
                    line.contains(UNKNOWN),
                    "{label} at {width} summed around a hole: {line:?}"
                );
                assert!(!line.contains(" mm"), "{label} at {width}: {line:?}");
            }
        }
    }

    /// Every value has to fit beside its label or it is clipped mid-word, and
    /// metric is the system that overflows first — a real bug the daily pane
    /// only caught because its test covered both.
    #[test]
    fn detail_values_fit_the_column() {
        let room = (DETAIL_WIDTH - 12) as usize;
        let mut hours = dry_hours(48);

        // A tropical-storm hour, and a blizzard hour, in the same series.
        for hour in hours.iter_mut() {
            hour.precip_mm = Some(120.5);
            hour.chance = Some(100);
            hour.temp_c = Some(-40.0);
        }
        hours[11].snow_cm = Some(45.7);

        for unit in [Unit::Metric, Unit::Imperial] {
            for selected in [0usize, 11, 47] {
                for line in detail_lines(&hours, selected, palette(), unit) {
                    let width = line.width();
                    assert!(
                        width <= room + 12,
                        "{unit:?} hour {selected}: {:?} is {width} wide, column allows {}",
                        line.to_string(),
                        room + 12
                    );
                }
            }
        }
    }

    fn rendered(width: u16, height: u16, app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| classic_render(f, app, palette(), f.area()))
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Chance belongs in one place: the hero. The first draft put it in the
    /// hero, the border and a detail row at once, which read as noise. The
    /// "24 h peak" row states a percentage too, but that is a different fact
    /// about a different hour — one occurrence, not three.
    #[test]
    fn the_chance_is_stated_once() {
        let mut hours = dry_hours(48);
        for hour in hours.iter_mut() {
            hour.chance = Some(53);
        }
        let app = app_showing(hours, 0);
        let text = rendered(100, 16, &app);

        assert!(
            text.contains("███"),
            "the hero draws the chance as block digits:\n{text}"
        );
        assert!(
            text.matches("53%").count() <= 1,
            "the chance is restated {} times:\n{text}",
            text.matches("53%").count()
        );
    }

    /// The border used to carry the chance beside the condition, which made
    /// three copies of one number.
    #[test]
    fn the_border_carries_the_condition_and_not_the_chance() {
        let mut hours = dry_hours(48);
        for hour in hours.iter_mut() {
            hour.chance = Some(53);
            hour.code = Some(61);
        }
        let app = app_showing(hours, 0);
        let text = rendered(100, 16, &app);
        let top = text.lines().next().expect("a top border");

        assert!(
            top.contains("rain"),
            "the condition rides the border: {top:?}"
        );
        assert!(!top.contains('%'), "the chance does not: {top:?}");
    }

    /// The same inspector frame as the weathergram screen: the title sits one
    /// cell in from the corner, and the city yields to the condition when the
    /// border runs short. The two views used to disagree on both.
    #[test]
    fn the_inspector_frame_matches_the_weathergram_screen() {
        let mut hours = dry_hours(48);
        for hour in hours.iter_mut() {
            hour.code = Some(96);
        }
        let app = app_showing(hours, 0);

        let wide = rendered(100, 16, &app);
        let top = wide.lines().next().expect("a top border");
        assert!(top.starts_with("┌ FREDERICK"), "{top:?}");

        let narrow = rendered(36, 16, &app);
        let top = narrow.lines().next().expect("a top border");
        assert!(
            top.contains("Thunderstorm"),
            "the condition should survive the squeeze: {top:?}"
        );
        assert!(
            top.contains('…'),
            "the city should be the one clipped: {top:?}"
        );
    }

    #[test]
    fn the_pane_keeps_its_readings_and_the_chart_gets_the_rest() {
        let app = app_showing(dry_hours(192), 0);
        let text = rendered(100, 24, &app);

        for label in ["amount", "temperature", "24 h total", "wet hours"] {
            assert!(text.contains(label), "lost {label:?}:\n{text}");
        }
        assert!(text.contains("FREDERICK"), "\n{text}");
        assert!(text.contains("Precipitation"), "the chart box:\n{text}");
    }

    /// The awkward sizes, including the app's declared minimum and either side
    /// of the height where the chart stops fitting.
    #[test]
    fn renders_without_panicking_at_awkward_sizes() {
        for hours in [dry_hours(0), dry_hours(1), dry_hours(192)] {
            for selected in [0usize, 1, 191] {
                let app = app_showing(hours.clone(), selected);
                for (width, height) in [
                    (34, 12),
                    (34, 11),
                    (40, 7),
                    (60, 12),
                    (80, 13),
                    (100, 24),
                    (200, 50),
                    (1, 1),
                ] {
                    let _ = rendered(width, height, &app);
                }
            }
        }
    }

    /// The pane is sized to its content whatever the terminal does, so a short
    /// window keeps every reading and the chart takes what is left.
    #[test]
    fn the_pane_keeps_its_size_when_the_chart_cannot_fit() {
        let app = app_showing(dry_hours(192), 0);

        for height in [inspector::ROWS, inspector::ROWS + 2, inspector::ROWS + 4] {
            let text = rendered(80, height, &app);
            for label in ["amount", "temperature", "wet hours"] {
                assert!(
                    text.contains(label),
                    "height {height} lost {label}:\n{text}"
                );
            }
            // The pane's own border must close where its content ends rather
            // than stretching around blank rows.
            assert!(
                text.lines()
                    .nth(inspector::ROWS as usize - 1)
                    .is_some_and(|l| l.starts_with('└')),
                "height {height}: the pane did not close at its own height:\n{text}"
            );
        }
    }

    #[test]
    fn the_selected_hour_is_named_on_the_border() {
        let app = app_showing(dry_hours(48), 14);
        let text = rendered(100, 16, &app);

        assert!(
            text.contains("2:00 PM"),
            "hour 14 of 10 Aug is 2 PM:\n{text}"
        );
    }
}
