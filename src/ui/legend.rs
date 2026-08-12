use crate::app::{App, Fetch, Screen};
use crate::theme::Palette;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Columns of indent, and the gap between one binding and the next.
const INDENT: usize = 2;
const SPACING: usize = 3;

/// Rows the legend may take before it starts dropping bindings. Two covers
/// every screen at any width the app renders at; past that it would be eating
/// rows the chart needs more.
const MAX_ROWS: u16 = 2;

/// The bar doubles as the theme readout: `t` is labelled with the palette
/// currently in use, so pressing it is its own feedback. That is why this
/// returns owned rows rather than a static slice — the label varies. Nothing
/// is allocated per name; theme names are static strings.
///
/// `t` goes last on both screens because the wrapping below drops from the
/// tail, and of everything here the palette is what a cramped terminal can
/// most afford to lose.
fn bindings(app: &App) -> Vec<(&'static str, &'static str)> {
    match app.screen {
        Screen::Weather => vec![
            ("q", "quit"),
            ("←→", "day"),
            ("n", "now"),
            ("p", "precip"),
            ("r", "refresh"),
            ("u", "units"),
            ("l", "location"),
            ("t", app.theme.name()),
        ],
        // Quit and back lead, as they do on the weather screen: if anything is
        // going to be dropped, those are the two worth keeping.
        Screen::Precipitation => vec![
            ("q", "quit"),
            ("b", "back"),
            ("←→", "hour"),
            ("↑↓", "day"),
            ("n", "now"),
            ("r", "refresh"),
            ("u", "units"),
            ("t", app.theme.name()),
        ],
        Screen::Search => match &app.results {
            Fetch::Ready(l) if !l.is_empty() => {
                vec![("↑↓", "navigate"), ("enter", "select"), ("esc", "cancel")]
            }
            _ => vec![("enter", "search"), ("esc", "cancel")],
        },
    }
}

/// Wrap the bindings onto as many rows as they need, up to `MAX_ROWS`.
///
/// One row that simply clipped sheared bindings mid-word as the terminal
/// narrowed, leaving something like `[u] uni` against the edge that read as a
/// rendering fault. Breaking between whole bindings keeps every one that is
/// shown legible, and lets a narrow terminal keep them all rather than losing
/// the tail.
fn wrapped(app: &App, palette: Palette, width: u16) -> Vec<Line<'static>> {
    let room = width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut row: Vec<Span> = Vec::new();
    let mut used = INDENT;

    for (key, label) in bindings(app) {
        let entry = format!("[{key}] {label}").chars().count();

        // Wider than the whole bar even on a row of its own. Nothing can be
        // shown without shearing it, so show nothing. Only reachable below the
        // app's minimum width, where the size warning replaces the interface.
        if INDENT + entry > room {
            continue;
        }

        // Only the gap *between* bindings counts; the first on a row is flush
        // against the indent.
        let needed = if row.is_empty() {
            entry
        } else {
            SPACING + entry
        };

        if !row.is_empty() && used + needed > room {
            if lines.len() + 1 >= MAX_ROWS as usize {
                // No rows left to give, so the remaining bindings go unshown
                // rather than overflowing the area.
                break;
            }
            lines.push(Line::from(std::mem::take(&mut row)));
            used = INDENT;
        }

        if row.is_empty() {
            row.push(Span::from(" ".repeat(INDENT)));
        } else {
            row.push(Span::from(" ".repeat(SPACING)));
            used += SPACING;
        }

        row.push(Span::from(format!("[{key}]")).fg(palette.selection));
        row.push(Span::from(format!(" {label}")).fg(palette.muted));
        used += entry;
    }

    if !row.is_empty() {
        lines.push(Line::from(row));
    }
    lines
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
    use crate::theme::Theme;
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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
    fn the_weather_legend_advertises_the_precipitation_screen() {
        let legend = legend_at(120, Screen::Weather).join("\n");
        assert!(legend.contains("[p]"), "{legend:?}");
        assert!(legend.contains("precip"), "{legend:?}");
    }

    /// The precipitation screen rebinds the arrows and takes `b` for back, so
    /// its legend must not still describe the weather screen's bindings.
    #[test]
    fn the_precipitation_legend_describes_its_own_keys() {
        let legend = legend_at(120, Screen::Precipitation).join("\n");

        assert!(legend.contains("[b] back"), "{legend:?}");
        assert!(legend.contains("hour"), "{legend:?}");
        assert!(
            !legend.contains("location"),
            "l returns to search: {legend:?}"
        );
    }

    /// The bar is the only thing that says which palette is on, so the label
    /// on `t` has to be the palette itself rather than the word "theme".
    #[test]
    fn the_bar_names_the_theme_that_is_on() {
        let mut app = app_on(Screen::Weather);

        let before = legend_at_with(&app, 120).join(" ");
        assert!(
            before.contains(&format!("[t] {}", app.theme.name())),
            "{before:?}"
        );

        app.theme = app.theme.next();
        let after = legend_at_with(&app, 120).join(" ");
        assert!(
            after.contains(&format!("[t] {}", app.theme.name())),
            "the bar did not follow the theme: {after:?}"
        );
        assert_ne!(before, after, "cycling left the bar unchanged");
    }

    /// The bug: a single row simply clipped, so narrowing the terminal sheared
    /// bindings mid-word. Every binding that appears must appear whole.
    ///
    /// Swept across every theme as well as every width, because the label on
    /// `t` is the theme's own name: the longest of them is what would shear
    /// first, and it is the only binding whose width is not fixed.
    #[test]
    fn narrowing_never_cuts_a_binding_in_half() {
        for theme in Theme::ALL {
            for screen in [Screen::Weather, Screen::Precipitation, Screen::Search] {
                for width in 8u16..=160 {
                    let mut app = app_on(screen);
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
        let app = app_on(Screen::Precipitation);
        assert_eq!(legend_rows(&app, 120), 1, "one row is plenty at 120");

        let narrow = legend_at(50, Screen::Precipitation);
        assert_eq!(narrow.len(), 2, "50 columns needs two rows: {narrow:?}");

        let shown = narrow.join(" ");
        for key in ["[q]", "[b]", "[←→]", "[↑↓]", "[n]"] {
            assert!(shown.contains(key), "50 columns lost {key}: {shown:?}");
        }
    }

    /// Two rows is the ceiling however little room there is, or the legend
    /// would start taking rows the chart needs more.
    #[test]
    fn the_legend_never_takes_more_than_its_share() {
        for width in 1u16..=200 {
            for screen in [Screen::Weather, Screen::Precipitation] {
                let rows = legend_rows(&app_on(screen), width);
                assert!((1..=MAX_ROWS).contains(&rows), "width {width}: {rows} rows");
            }
        }
    }

    /// Whatever gets dropped, the two keys that get you out come first.
    #[test]
    fn quitting_and_leaving_survive_a_very_narrow_terminal() {
        let legend = legend_at(20, Screen::Precipitation).join(" ");
        assert!(legend.contains("[q]"), "{legend:?}");
        assert!(legend.contains("[b]"), "{legend:?}");
    }
}
