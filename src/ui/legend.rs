use crate::app::{App, Fetch, Screen};
use crate::theme::Palette;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Columns between one binding and the next.
const SPACING: usize = 3;

/// Rows the legend may take before it starts dropping bindings. Two covers
/// every screen at any width the app renders at; past that it would be eating
/// rows the chart needs more.
const MAX_ROWS: u16 = 2;

/// What `t` is labelled with: the word, and — for a few seconds after it is
/// pressed — the palette it landed on, in brackets after it.
///
/// The name alone, `[t] nord`, said what was on but not what the key did,
/// which is the one job every other label on the bar has. The name is the
/// answer to "which one did I just get", so it is worth showing while that is
/// a live question and not after: left up it is a readout nobody is reading,
/// and on a narrow terminal it costs a whole binding its place.
fn theme_label(app: &App) -> String {
    if app.theme_readout_visible() {
        format!("theme ({})", app.theme.name())
    } else {
        "theme".to_string()
    }
}

/// The bar doubles as the theme readout: `t` carries the palette currently in
/// use, so pressing it is its own feedback. That is why this returns owned
/// rows rather than a static slice — one label varies.
///
/// `t` goes last on both screens because the wrapping below drops from the
/// tail, and of everything here the palette is what a cramped terminal can
/// most afford to lose.
fn bindings(app: &App) -> Vec<(&'static str, String)> {
    let owned = |pairs: Vec<(&'static str, &str)>| -> Vec<(&'static str, String)> {
        pairs
            .into_iter()
            .map(|(key, label)| (key, label.to_string()))
            .collect()
    };

    match app.screen {
        Screen::Weather => owned(vec![
            ("q", "quit"),
            ("←→", "day"),
            ("n", "now"),
            ("p", "hourly"),
            ("r", "refresh"),
            ("u", "units"),
            ("l", "location"),
            ("t", &theme_label(app)),
        ]),
        // Quit and back lead, as they do on the weather screen: if anything is
        // going to be dropped, those are the two worth keeping.
        Screen::Hourly => owned(vec![
            ("q", "quit"),
            ("b", "back"),
            ("←→", "hour"),
            ("↑↓", "day"),
            ("n", "now"),
            ("v", "view"),
            ("r", "refresh"),
            ("u", "units"),
            ("t", &theme_label(app)),
        ]),
        Screen::Search => match &app.results {
            Fetch::Ready(l) if !l.is_empty() => owned(vec![
                ("↑↓", "navigate"),
                ("enter", "select"),
                ("esc", "cancel"),
            ]),
            _ => owned(vec![("enter", "search"), ("esc", "cancel")]),
        },
    }
}

/// Lay the bindings out on up to `MAX_ROWS` centred rows.
///
/// One row that simply clipped sheared bindings mid-word as the terminal
/// narrowed, leaving something like `[u] uni` against the edge that read as a
/// rendering fault. Breaking between whole bindings keeps every one that is
/// shown legible, and lets a narrow terminal keep them all rather than losing
/// the tail. The rows are centred, so the bar sits under the centred panes
/// above it instead of pinned to the left edge, and `split` levels the break
/// so a wrapped bar reads as two deliberate rows rather than a full one over
/// a stub.
fn wrapped(app: &App, palette: Palette, width: u16) -> Vec<Line<'static>> {
    let room = width as usize;

    // A binding wider than the whole bar cannot be shown without shearing
    // it, so it is not shown at all. Only reachable below the app's minimum
    // width, where the size warning replaces the interface.
    let entry = |key: &str, label: &str| format!("[{key}] {label}").chars().count();
    let bindings: Vec<(&'static str, String)> = bindings(app)
        .into_iter()
        .filter(|(key, label)| entry(key, label) <= room)
        .collect();
    let widths: Vec<usize> = bindings
        .iter()
        .map(|(key, label)| entry(key, label))
        .collect();

    let mut lines: Vec<Line> = Vec::new();
    let mut next = 0;
    for count in split(&widths, room) {
        let mut row: Vec<Span> = Vec::new();
        for (key, label) in &bindings[next..next + count] {
            if !row.is_empty() {
                row.push(Span::from(" ".repeat(SPACING)));
            }
            row.push(Span::from(format!("[{key}]")).fg(palette.selection));
            row.push(Span::from(format!(" {label}")).fg(palette.muted));
        }
        next += count;
        lines.push(Line::from(row).centered());
    }
    lines
}

/// How many bindings go on each row.
///
/// Greedy packing decides how many are shown at all: each row is filled
/// before the next begins, so as many bindings are kept as `MAX_ROWS` rows
/// can hold, and the rest drop from the tail. But a greedy break leaves the
/// first row full to the brim over a second holding whatever fell off it,
/// so once the count is settled the break moves to wherever levels the two
/// rows most evenly. The greedy break itself always qualifies, so there is
/// always a candidate.
fn split(widths: &[usize], room: usize) -> Vec<usize> {
    let mut rows: Vec<usize> = Vec::new();
    let mut index = 0;
    while index < widths.len() && rows.len() < MAX_ROWS as usize {
        let mut count = 0;
        let mut used = 0;
        while index < widths.len() {
            let needed = if count == 0 {
                widths[index]
            } else {
                used + SPACING + widths[index]
            };
            if count > 0 && needed > room {
                break;
            }
            used = needed;
            count += 1;
            index += 1;
        }
        rows.push(count);
    }

    let [first, second] = rows[..] else {
        // One row, or none: nothing to level.
        return rows;
    };

    let kept = first + second;
    let widths = &widths[..kept];
    let widest = |at: usize| row_width(&widths[..at]).max(row_width(&widths[at..]));

    let mut best = first;
    for candidate in 1..kept {
        if row_width(&widths[..candidate]) <= room
            && row_width(&widths[candidate..]) <= room
            && widest(candidate) < widest(best)
        {
            best = candidate;
        }
    }
    vec![best, kept - best]
}

/// Columns a run of bindings takes on one row, gaps included.
fn row_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + SPACING * widths.len().saturating_sub(1)
}

/// How many rows the legend needs at this width, so the caller can reserve
/// them before laying out everything else.
pub(super) fn legend_rows(app: &App, width: u16) -> u16 {
    // The palette cannot change how much room the bindings need, so measuring
    // with any of them gives the same answer.
    (wrapped(app, app.theme.palette(), width).len() as u16).clamp(1, MAX_ROWS)
}

pub(super) fn keybind_legend_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    frame.render_widget(Paragraph::new(wrapped(app, palette, area.width)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Action;
    use crate::theme::Theme;
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::{Duration, Instant};

    fn app_on(screen: Screen) -> App {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.screen = screen;
        app
    }

    fn legend_at(width: u16, screen: Screen) -> Vec<String> {
        legend_at_with(&app_on(screen), width)
    }

    fn legend_at_with(app: &App, width: u16) -> Vec<String> {
        let height = legend_rows(app, width);

        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let palette = app.theme.palette();
        terminal
            .draw(|f| keybind_legend_render(f, app, palette, f.area()))
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    /// Nothing else on the weather screen mentions that `p` exists, so the
    /// legend is the only thing making the screen discoverable at all.
    #[test]
    fn the_weather_legend_advertises_the_hourly_screen() {
        let legend = legend_at(120, Screen::Weather).join("\n");
        assert!(legend.contains("[p] hourly"), "{legend:?}");
    }

    /// The hourly screen rebinds the arrows and takes `b` for back, so
    /// its legend must not still describe the weather screen's bindings.
    #[test]
    fn the_hourly_legend_describes_its_own_keys() {
        let legend = legend_at(120, Screen::Hourly).join("\n");

        assert!(legend.contains("[b] back"), "{legend:?}");
        assert!(legend.contains("hour"), "{legend:?}");
        assert!(legend.contains("[v] view"), "{legend:?}");
        assert!(
            !legend.contains("location"),
            "l returns to search: {legend:?}"
        );
    }

    /// Pressing `t` has to be its own feedback: the bar is the only thing on
    /// screen that says which palette you just landed on.
    #[test]
    fn pressing_the_key_names_the_palette_it_landed_on() {
        let mut app = app_on(Screen::Weather);

        app.on_action(Action::CycleTheme);
        let first = legend_at_with(&app, 120).join(" ");
        assert!(
            first.contains(&format!("[t] theme ({})", app.theme.name())),
            "{first:?}"
        );

        app.on_action(Action::CycleTheme);
        let second = legend_at_with(&app, 120).join(" ");
        assert!(
            second.contains(&format!("[t] theme ({})", app.theme.name())),
            "the bar did not follow the theme: {second:?}"
        );
        assert_ne!(first, second, "cycling left the bar unchanged");
    }

    /// Whichever palette is on, the readout that names it fits the bar. The
    /// name is the only label here whose width is not fixed, so the longest of
    /// them is what would shear first.
    #[test]
    fn every_palette_can_be_named_on_the_bar() {
        for theme in Theme::ALL {
            let mut app = app_on(Screen::Weather);
            app.theme = theme;
            app.on_action(Action::CycleTheme);

            let legend = legend_at_with(&app, 120).join(" ");
            assert!(
                legend.contains(&format!("[t] theme ({})", app.theme.name())),
                "{}: {legend:?}",
                app.theme.name()
            );
        }
    }

    /// The key keeps its label once the name has gone — `t` is still a binding
    /// like any other, it has just stopped answering a question nobody is
    /// asking any more.
    #[test]
    fn the_name_goes_but_the_binding_stays() {
        let mut app = app_on(Screen::Weather);

        // Before it has ever been pressed there is nothing to report.
        let untouched = legend_at_with(&app, 120).join(" ");
        assert!(untouched.contains("[t] theme"), "{untouched:?}");
        assert!(
            !untouched.contains("[t] theme ("),
            "the bar named a palette nobody had asked about: {untouched:?}"
        );

        app.on_action(Action::CycleTheme);
        assert!(legend_at_with(&app, 120).join(" ").contains("[t] theme ("));

        app.expire_theme_readout(Instant::now() + Duration::from_secs(60));
        let lapsed = legend_at_with(&app, 120).join(" ");
        assert!(lapsed.contains("[t] theme"), "{lapsed:?}");
        assert!(
            !lapsed.contains("[t] theme ("),
            "the name outstayed its welcome: {lapsed:?}"
        );
    }

    /// The readout is transient, so the room it takes has to come back — on a
    /// narrow terminal it is the difference between the bar fitting on one row
    /// and taking a row off the chart.
    #[test]
    fn the_bar_gets_its_columns_back() {
        let mut app = app_on(Screen::Weather);
        app.theme = Theme::GruvboxDark;
        app.on_action(Action::CycleTheme);

        let showing = legend_at_with(&app, 120).join(" ").trim_end().len();

        app.expire_theme_readout(Instant::now() + Duration::from_secs(60));
        let hidden = legend_at_with(&app, 120).join(" ").trim_end().len();

        assert!(
            hidden < showing,
            "the name went but its columns did not: {hidden} vs {showing}"
        );
    }

    /// The bug: a single row simply clipped, so narrowing the terminal sheared
    /// bindings mid-word. Every binding that appears must appear whole.
    ///
    /// Swept across every theme as well as every width, with the readout
    /// showing: `[t] theme (gruvbox dark)` is the widest label the bar
    /// ever carries, and it is the only one whose width is not fixed. Sweeping
    /// with the readout hidden would test the *narrowest* case and call it
    /// covered.
    #[test]
    fn narrowing_never_cuts_a_binding_in_half() {
        for theme in Theme::ALL {
            for screen in [Screen::Weather, Screen::Hourly, Screen::Search] {
                for width in 8u16..=160 {
                    let mut app = app_on(screen);
                    // Press first, then force the palette: the press is what
                    // puts the name on the bar, and cycling would otherwise
                    // land on a different theme than the one under test.
                    app.on_action(Action::CycleTheme);
                    app.theme = theme;
                    let rows = legend_at_with(&app, width);

                    for row in &rows {
                        assert!(
                            row.chars().count() <= width as usize,
                            "{} at {width}: row overflows: {row:?}",
                            theme.name()
                        );
                        // A sheared binding leaves an opening bracket with no
                        // closing one.
                        assert_eq!(
                            row.matches('[').count(),
                            row.matches(']').count(),
                            "{} at {width}: cut a key in half: {row:?}",
                            theme.name()
                        );
                    }

                    // Any key that made it onto the bar brought its label with it.
                    let shown = rows.join(" ");
                    for (key, label) in bindings(&app) {
                        if shown.contains(&format!("[{key}]")) {
                            assert!(
                                shown.contains(&format!("[{key}] {label}")),
                                "{} at {width}: {key:?} lost its label: {shown:?}",
                                theme.name()
                            );
                        }
                    }
                }
            }
        }
    }

    /// Wrapping is what lets a narrow terminal keep bindings it would
    /// otherwise have clipped off the end.
    #[test]
    fn a_narrow_terminal_wraps_rather_than_dropping_the_tail() {
        let app = app_on(Screen::Hourly);
        assert_eq!(legend_rows(&app, 120), 1, "one row is plenty at 120");

        let narrow = legend_at(50, Screen::Hourly);
        assert_eq!(narrow.len(), 2, "50 columns needs two rows: {narrow:?}");

        let shown = narrow.join(" ");
        for key in ["[q]", "[b]", "[←→]", "[↑↓]", "[n]"] {
            assert!(shown.contains(key), "50 columns lost {key}: {shown:?}");
        }
    }

    /// The complaint: the bar sat pinned to the left edge while every pane
    /// above it is centred, so it read as detached from the interface.
    #[test]
    fn the_bar_is_centred_rather_than_pinned_left() {
        for screen in [Screen::Weather, Screen::Hourly, Screen::Search] {
            for row in legend_at(120, screen) {
                let leading = row.chars().take_while(|c| *c == ' ').count();
                let trailing = row.chars().rev().take_while(|c| *c == ' ').count();
                assert!(leading > 0, "{screen:?}: {row:?}");
                assert!(
                    leading.abs_diff(trailing) <= 1,
                    "{screen:?}: {leading} blank columns left, {trailing} right: {row:?}"
                );
            }
        }
    }

    /// The other half of the complaint: a bar that wrapped greedily left the
    /// first row full to the brim over a stub of whatever fell off it. The
    /// break moves to level the rows — at this width, greedy would leave two
    /// bindings below seven.
    #[test]
    fn a_wrapped_bar_levels_its_rows() {
        let app = app_on(Screen::Hourly);
        let rows = legend_at_with(&app, 80);
        assert_eq!(rows.len(), 2, "80 columns should wrap: {rows:?}");
        assert!(
            rows[1].matches('[').count() >= 3,
            "the wrap left a stub: {rows:?}"
        );

        // At a levelled break the rows sit within one binding of each other:
        // were they further apart, moving the boundary binding down would
        // have levelled them more.
        let widest = bindings(&app)
            .iter()
            .map(|(key, label)| format!("[{key}] {label}").chars().count())
            .max()
            .unwrap();
        let width = |row: &String| row.trim().chars().count();
        assert!(
            width(&rows[0]).abs_diff(width(&rows[1])) <= widest + SPACING,
            "{rows:?}"
        );
    }

    #[test]
    fn hourly_legend_keeps_exit_keys_within_the_minimum_height_budget() {
        let app = app_on(Screen::Hourly);
        assert_eq!(legend_rows(&app, 34), MAX_ROWS);

        let legend = legend_at(34, Screen::Hourly).join(" ");
        for key in ["[q]", "[b]"] {
            assert!(
                legend.contains(key),
                "minimum-width legend lost {key}: {legend:?}"
            );
        }
    }

    /// Two rows is the ceiling however little room there is, or the legend
    /// would start taking rows the chart needs more.
    #[test]
    fn the_legend_never_takes_more_than_its_share() {
        for width in 1u16..=200 {
            for screen in [Screen::Weather, Screen::Hourly] {
                let rows = legend_rows(&app_on(screen), width);
                assert!((1..=MAX_ROWS).contains(&rows), "width {width}: {rows} rows");
            }
        }
    }

    /// Whatever gets dropped, the two keys that get you out come first.
    #[test]
    fn quitting_and_leaving_survive_a_very_narrow_terminal() {
        let legend = legend_at(20, Screen::Hourly).join(" ");
        assert!(legend.contains("[q]"), "{legend:?}");
        assert!(legend.contains("[b]"), "{legend:?}");
    }
}
