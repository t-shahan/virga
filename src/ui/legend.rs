use crate::app::{App, Fetch, KeyHintStyle, Screen};
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

fn owned(pairs: Vec<(&'static str, &str)>) -> Vec<(&'static str, String)> {
    pairs
        .into_iter()
        .map(|(key, label)| (key, label.to_string()))
        .collect()
}

/// Every binding the screen answers to — the overlay's reading matter, no
/// longer the bar's. `?` closes as well as opens, so it earns a line like
/// any other key.
pub(super) fn bindings(app: &App) -> Vec<(&'static str, String)> {
    // `?` only does something in `Hint` style — `Full` already names
    // everything here, so listing a binding for a key that does nothing
    // would be a lie the reference itself is telling.
    let help = (app.key_hint_style == KeyHintStyle::Hint).then_some(("?", "keys".to_string()));

    match app.screen {
        Screen::Weather => owned(vec![
            ("q", "quit"),
            ("←→↑↓", "day"),
            ("n", "now"),
            ("p", "hourly"),
            ("r", "refresh"),
            ("u", "units"),
            ("l", "location"),
            ("t", &theme_label(app)),
            (",", "hide"),
        ])
        .into_iter()
        .chain(help)
        .collect(),
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
            (",", "hide"),
        ])
        .into_iter()
        .chain(help)
        .collect(),
        // Search never opens the overlay — `?` is text there — so its short
        // self-explanatory lists are both hint and reference.
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

/// What the bar itself shows: the way out, the way back where there is one,
/// and the key that opens the full reference. The bar still doubles as the
/// theme readout — `t`'s feedback has nowhere else to be seen, since the
/// overlay is closed while anyone is cycling — but only while the readout is
/// live; the standing entry lives in the overlay.
fn hint(app: &App) -> Vec<(&'static str, String)> {
    let mut pairs = match app.screen {
        Screen::Weather => owned(vec![("q", "quit"), ("?", "keybinds")]),
        Screen::Hourly => owned(vec![("q", "quit"), ("b", "back"), ("?", "keybinds")]),
        Screen::Search => return bindings(app),
    };
    if app.theme_readout_visible() {
        pairs.push(("t", theme_label(app)));
    }
    pairs
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
fn wrapped(
    bindings: Vec<(&'static str, String)>,
    palette: Palette,
    width: u16,
) -> Vec<Line<'static>> {
    let room = width as usize;

    // A binding wider than the whole bar cannot be shown without shearing
    // it, so it is not shown at all. Only reachable below the app's minimum
    // width, where the size warning replaces the interface.
    let entry = |key: &str, label: &str| format!("[{key}] {label}").chars().count();
    let bindings: Vec<(&'static str, String)> = bindings
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
///
/// Every width must already fit in `room` on its own — `wrapped` filters
/// before calling. An over-wide entry would not panic here, but the
/// first-on-a-row branch accepts it unconditionally and the row would
/// silently clip.
fn split(widths: &[usize], room: usize) -> Vec<usize> {
    // The levelling pass below destructures exactly two rows and skips any
    // other count. Raising MAX_ROWS means teaching it to level more rows,
    // not just changing the constant.
    const _: () = assert!(MAX_ROWS == 2);

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

/// What the bar shows: every binding in `Full` style, since there is no card
/// behind `?` to carry the rest, and just the hint in `Hint` style.
fn displayed(app: &App) -> Vec<(&'static str, String)> {
    match app.key_hint_style {
        KeyHintStyle::Full => bindings(app),
        KeyHintStyle::Hint => hint(app),
    }
}

/// How many rows the legend needs at this width, so the caller can reserve
/// them before laying out everything else.
pub(super) fn legend_rows(app: &App, width: u16) -> u16 {
    // The palette cannot change how much room the bindings need, so measuring
    // with any of them gives the same answer.
    (wrapped(displayed(app), app.theme.palette(), width).len() as u16).clamp(1, MAX_ROWS)
}

pub(super) fn keybind_legend_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    frame.render_widget(
        Paragraph::new(wrapped(displayed(app), palette, area.width)),
        area,
    );
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

    /// `Full` style is what a user opts into to get the old always-visible
    /// bar back — every binding named, the same as `bindings` gives the
    /// overlay in `Hint` style.
    #[test]
    fn full_style_lists_every_binding_on_the_bar_itself() {
        let mut app = app_on(Screen::Weather);
        app.key_hint_style = KeyHintStyle::Full;
        let legend = legend_at_with(&app, 120).join("\n");
        for entry in ["[p] hourly", "[l] location", "[t] theme"] {
            assert!(legend.contains(entry), "{legend:?}");
        }
    }

    /// `?` opens nothing in `Full` style, so advertising it on the bar would
    /// be a binding that lies about what pressing it does.
    #[test]
    fn full_style_never_advertises_the_question_mark() {
        let mut app = app_on(Screen::Weather);
        app.key_hint_style = KeyHintStyle::Full;
        let legend = legend_at_with(&app, 120).join("\n");
        assert!(!legend.contains("[?]"), "{legend:?}");
    }

    /// The toggle itself has to be discoverable from whichever style is
    /// active: from `Hint` behind the card, and from `Full` right on the bar,
    /// since `Full` has nowhere else to put it.
    #[test]
    fn the_key_style_toggle_is_listed_in_both_styles() {
        for style in [KeyHintStyle::Hint, KeyHintStyle::Full] {
            let mut app = app_on(Screen::Weather);
            app.key_hint_style = style;
            let listed = bindings(&app).iter().any(|(key, _)| *key == ",");
            assert!(listed, "{style:?}");
        }
    }

    /// The bar is a hint now, so the one thing it must advertise is the key
    /// that shows everything else — and the way out, which is too important
    /// to hide behind another keypress.
    #[test]
    fn the_bar_hints_at_the_full_reference() {
        for screen in [Screen::Weather, Screen::Hourly] {
            let legend = legend_at(120, screen).join("\n");
            assert!(legend.contains("[q] quit"), "{screen:?}: {legend:?}");
            assert!(legend.contains("[?] keybinds"), "{screen:?}: {legend:?}");
        }
    }

    /// The full list lives behind `?`; a bar still carrying it would mean
    /// the overlay freed no rows.
    #[test]
    fn the_bar_no_longer_carries_the_full_list() {
        let weather = legend_at(120, Screen::Weather).join("\n");
        assert!(!weather.contains("[p] hourly"), "{weather:?}");

        let hourly = legend_at(120, Screen::Hourly).join("\n");
        assert!(!hourly.contains("[v] view"), "{hourly:?}");
    }

    /// `b` stays on the hourly hint: how you got in is not obvious once you
    /// are there, and the way back should not take a detour through help.
    #[test]
    fn the_hourly_hint_keeps_a_way_back() {
        let legend = legend_at(120, Screen::Hourly).join("\n");
        assert!(legend.contains("[b] back"), "{legend:?}");
    }

    /// The point of the hint: one row even at the minimum width, where the
    /// full list used to wrap onto two and cost the chart a row.
    #[test]
    fn the_hint_fits_one_row_even_at_the_minimum_width() {
        for screen in [Screen::Weather, Screen::Hourly] {
            assert_eq!(legend_rows(&app_on(screen), 34), 1, "{screen:?}");
        }
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

    /// `t` on the hint is feedback, not a standing entry: the name answers
    /// "which one did I just get" while that is a live question, and the
    /// whole binding steps back into the overlay once it has been read.
    #[test]
    fn the_theme_readout_visits_the_hint_and_leaves() {
        let mut app = app_on(Screen::Weather);

        // Before it has ever been pressed there is nothing to report.
        let untouched = legend_at_with(&app, 120).join(" ");
        assert!(
            !untouched.contains("[t]"),
            "the hint named a palette nobody had asked about: {untouched:?}"
        );

        app.on_action(Action::CycleTheme);
        assert!(legend_at_with(&app, 120).join(" ").contains("[t] theme ("));

        app.expire_theme_readout(Instant::now() + Duration::from_secs(60));
        let lapsed = legend_at_with(&app, 120).join(" ");
        assert!(
            !lapsed.contains("[t]"),
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
                    for (key, label) in hint(&app) {
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

    /// `Full` style's list is longer than the hint's — the same sweep, over
    /// `bindings` instead, since that list is what the bar now carries whole
    /// rather than behind a card.
    #[test]
    fn narrowing_never_cuts_a_full_style_binding_in_half() {
        for screen in [Screen::Weather, Screen::Hourly, Screen::Search] {
            for width in 8u16..=160 {
                let mut app = app_on(screen);
                app.key_hint_style = KeyHintStyle::Full;
                let rows = legend_at_with(&app, width);

                for row in &rows {
                    assert!(
                        row.chars().count() <= width as usize,
                        "at {width}: row overflows: {row:?}"
                    );
                    assert_eq!(
                        row.matches('[').count(),
                        row.matches(']').count(),
                        "at {width}: cut a key in half: {row:?}"
                    );
                }

                let shown = rows.join(" ");
                for (key, label) in bindings(&app) {
                    if shown.contains(&format!("[{key}]")) {
                        assert!(
                            shown.contains(&format!("[{key}] {label}")),
                            "at {width}: {key:?} lost its label: {shown:?}"
                        );
                    }
                }
            }
        }
    }

    /// Wrapping is what lets a terminal narrower than the minimum keep the
    /// hint's bindings whole instead of clipping them mid-word.
    #[test]
    fn a_very_narrow_terminal_wraps_the_hint_rather_than_clipping() {
        let narrow = legend_at(20, Screen::Weather);
        assert_eq!(narrow.len(), 2, "20 columns needs two rows: {narrow:?}");

        let shown = narrow.join(" ");
        for key in ["[q]", "[?]"] {
            assert!(shown.contains(key), "20 columns lost {key}: {shown:?}");
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

    /// A greedy break leaves the first row full to the brim over a stub. The
    /// second pass moves it: four equal bindings at a width that greedily
    /// packs three-and-one must come out two-and-two.
    #[test]
    fn a_wrapped_split_levels_its_rows() {
        assert_eq!(split(&[10, 10, 10, 10], 46), vec![2, 2]);
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

    /// `Full` style's longer list is still bound by the same ceiling — the
    /// row it costs never grows past what `Hint` style's bar already reserves
    /// room for.
    #[test]
    fn full_style_never_takes_more_than_its_share_either() {
        for width in 1u16..=200 {
            for screen in [Screen::Weather, Screen::Hourly] {
                let mut app = app_on(screen);
                app.key_hint_style = KeyHintStyle::Full;
                let rows = legend_rows(&app, width);
                assert!((1..=MAX_ROWS).contains(&rows), "width {width}: {rows} rows");
            }
        }
    }
}
