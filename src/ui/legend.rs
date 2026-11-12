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

/// A key, or the keys that share a job, and what it does.
pub(super) type Binding = (&'static str, String);

/// Bindings under the heading the key card files them under.
pub(super) struct Group {
    pub heading: &'static str,
    pub bindings: Vec<Binding>,
}

/// What `t` is labelled with: the word, and for a few seconds after it is
/// pressed the palette it landed on, in brackets after it.
///
/// The name is the answer to "which one did I just get", so it is worth
/// showing while that is a live question and not after: left up it is a
/// readout nobody is reading, and on a narrow terminal it costs a whole
/// binding its place.
fn theme_label(app: &App) -> String {
    if app.theme_readout_visible() {
        format!("theme ({})", app.theme.name())
    } else {
        "theme".to_string()
    }
}

fn owned(pairs: &[(&'static str, &str)]) -> Vec<Binding> {
    pairs
        .iter()
        .map(|&(key, label)| (key, label.to_string()))
        .collect()
}

fn group(heading: &'static str, pairs: &[(&'static str, &str)]) -> Group {
    Group {
        heading,
        bindings: owned(pairs),
    }
}

/// Every key the screen answers to, for the card behind `?`. Keys that do
/// one job share a line, so the card is as long as the list of jobs and not
/// the list of keys.
pub(super) fn reference(app: &App) -> Vec<Group> {
    let pause = if app.playback.is_playing() {
        "pause"
    } else {
        "play"
    };
    let theme = theme_label(app);
    let app_keys = group(
        "App",
        &[
            ("r", "refresh"),
            ("u", "units"),
            ("t", &theme),
            ("?", "close this"),
            ("q", "quit"),
        ],
    );
    match app.screen {
        Screen::Weather => vec![
            group("Move", &[("←→↑↓", "day"), ("n", "today")]),
            group(
                "Go to",
                &[("h 2", "hourly"), ("m 3", "radar map"), ("/", "search")],
            ),
            app_keys,
        ],
        Screen::Hourly => vec![
            group("Move", &[("←→", "hour"), ("↑↓", "day"), ("n", "now")]),
            group("Show", &[("v", "weathergram or classic")]),
            group(
                "Go to",
                &[
                    ("esc 1 h", "forecast"),
                    ("m 3", "radar map"),
                    ("/", "search"),
                ],
            ),
            app_keys,
        ],
        Screen::Radar => vec![
            group(
                "Move",
                &[("←→↑↓", "pan"), ("+ -", "zoom"), ("n", "your city")],
            ),
            group("Loop", &[(", .", "frame"), ("space", pause)]),
            group(
                "Go to",
                &[("esc 1 m", "forecast"), ("h 2", "hourly"), ("/", "search")],
            ),
            app_keys,
        ],
        // Search never opens the card: `?` is text there.
        Screen::Search => Vec::new(),
    }
}

/// What the bar shows, in two parts: the screen's own keys, and the ones
/// that stay when there is not room for all of it.
///
/// The bar is a reminder and not the reference. It names what this screen
/// is for, and the three keys that get someone unstuck: back, the card with
/// everything on it, and out.
fn bar(app: &App) -> (Vec<Binding>, Vec<Binding>) {
    let pause = if app.playback.is_playing() {
        "pause"
    } else {
        "play"
    };
    let (own, back) = match app.screen {
        Screen::Weather => (
            owned(&[
                ("←→", "day"),
                ("h", "hourly"),
                ("m", "map"),
                ("/", "search"),
            ]),
            false,
        ),
        Screen::Hourly => (
            owned(&[("←→", "hour"), ("↑↓", "day"), ("v", "view"), ("n", "now")]),
            true,
        ),
        Screen::Radar => (
            owned(&[
                ("←→↑↓", "move"),
                ("+-", "zoom"),
                (",.", "frame"),
                ("space", pause),
                ("n", "home"),
            ]),
            true,
        ),
        Screen::Search => {
            let own = match &app.results {
                Fetch::Ready(found) if !found.is_empty() => {
                    owned(&[("↑↓", "navigate"), ("enter", "select"), ("esc", "cancel")])
                }
                _ => owned(&[("enter", "search"), ("esc", "cancel")]),
            };
            return (Vec::new(), own);
        }
    };

    let mut standing = Vec::new();
    if back {
        standing.push(("esc", "back".to_string()));
    }
    standing.push(("?", "keys".to_string()));
    standing.push(("q", "quit".to_string()));
    // `t`'s feedback has nowhere else to be seen, since the card is closed
    // while anyone is cycling, so it stands with the keys that are not
    // dropped for as long as it is live.
    if app.theme_readout_visible() {
        standing.push(("t", theme_label(app)));
    }
    (own, standing)
}

/// Columns a binding takes: its key, a space, its label.
fn entry_width((key, label): &Binding) -> usize {
    key.chars().count() + 1 + label.chars().count()
}

/// The bindings the bar has room for, in the order it shows them: the
/// screen's own first, then the standing ones.
///
/// What goes when the bar is too narrow is the tail of the screen's own
/// keys, so the middle of the bar gives way and both ends hold. A standing
/// binding wider than the whole bar cannot be shown without shearing it, so
/// it is not shown at all, which is only reachable below the app's minimum
/// width, where the size warning replaces the interface.
fn fitted(app: &App, width: u16) -> Vec<Binding> {
    let room = width as usize;
    let (own, standing) = bar(app);
    let fits = |binding: &Binding| entry_width(binding) <= room;
    let own: Vec<Binding> = own.into_iter().filter(fits).collect();
    let standing: Vec<Binding> = standing.into_iter().filter(fits).collect();

    for kept in (0..=own.len()).rev() {
        let shown: Vec<Binding> = own[..kept].iter().chain(&standing).cloned().collect();
        let widths: Vec<usize> = shown.iter().map(entry_width).collect();
        if split(&widths, room).iter().sum::<usize>() == shown.len() {
            return shown;
        }
    }
    standing
}

/// Lay the bindings out on up to `MAX_ROWS` centred rows.
///
/// Breaking between whole bindings keeps every one that is shown legible.
/// The rows are centred, so the bar sits under the centred panes above it,
/// and `split` levels the break so a wrapped bar reads as two deliberate
/// rows and not a full one over a stub.
///
/// A key is told from its label by colour alone. Brackets around it cost
/// two columns a binding, and made a key that is itself a bracket unreadable.
fn wrapped(bindings: &[Binding], palette: Palette, width: u16) -> Vec<Line<'static>> {
    let widths: Vec<usize> = bindings.iter().map(entry_width).collect();
    let mut lines: Vec<Line> = Vec::new();
    let mut next = 0;
    for count in split(&widths, width as usize) {
        let mut row: Vec<Span> = Vec::new();
        for (key, label) in &bindings[next..next + count] {
            if !row.is_empty() {
                row.push(Span::from(" ".repeat(SPACING)));
            }
            row.push(Span::from(*key).fg(palette.selection).bold());
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

/// How many rows the legend needs at this width, so the caller can reserve
/// them before laying out everything else.
pub(super) fn legend_rows(app: &App, width: u16) -> u16 {
    // The palette cannot change how much room the bindings need, so measuring
    // with any of them gives the same answer.
    (wrapped(&fitted(app, width), app.theme.palette(), width).len() as u16).clamp(1, MAX_ROWS)
}

pub(super) fn keybind_legend_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    frame.render_widget(
        Paragraph::new(wrapped(&fitted(app, area.width), palette, area.width)),
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

    const SCREENS: [Screen; 3] = [Screen::Weather, Screen::Hourly, Screen::Radar];

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

    /// The bindings a row of the bar shows, split where the bar spaces them.
    fn shown(rows: &[String]) -> Vec<String> {
        rows.iter()
            .flat_map(|row| row.trim().split("   "))
            .map(|binding| binding.trim().to_string())
            .filter(|binding| !binding.is_empty())
            .collect()
    }

    #[test]
    fn a_wide_bar_names_the_screens_keys_and_the_ways_out() {
        assert_eq!(
            shown(&legend_at(120, Screen::Weather)),
            [
                "←→ day",
                "h hourly",
                "m map",
                "/ search",
                "? keys",
                "q quit"
            ]
        );
        assert_eq!(
            shown(&legend_at(120, Screen::Hourly)),
            [
                "←→ hour",
                "↑↓ day",
                "v view",
                "n now",
                "esc back",
                "? keys",
                "q quit"
            ]
        );
        assert_eq!(
            shown(&legend_at(120, Screen::Radar)),
            [
                "←→↑↓ move",
                "+- zoom",
                ",. frame",
                "space pause",
                "n home",
                "esc back",
                "? keys",
                "q quit"
            ]
        );
    }

    /// No key on the bar is wrapped in brackets, which is what let a key
    /// that is a bracket or a comma be read at all.
    #[test]
    fn keys_are_set_off_by_colour_and_not_by_brackets() {
        let app = app_on(Screen::Radar);
        let palette = app.theme.palette();
        let lines = wrapped(&fitted(&app, 120), palette, 120);
        let text: String = lines.iter().map(Line::to_string).collect();
        assert!(!text.contains('[') && !text.contains(']'), "{text}");

        let spans = &lines[0].spans;
        assert_eq!(spans[0].content, "←→↑↓");
        assert_eq!(spans[0].style.fg, Some(palette.selection));
        assert_eq!(spans[1].content, " move");
        assert_eq!(spans[1].style.fg, Some(palette.muted));
    }

    /// As the terminal narrows the screen's own keys go from the end, and
    /// the way back, the card and the way out are still there at the
    /// narrowest the app draws.
    #[test]
    fn narrowing_drops_the_middle_and_keeps_both_ends() {
        for screen in SCREENS {
            let mut last = usize::MAX;
            for width in (34..=120).rev() {
                let rows = legend_at(width, screen);
                assert!(rows.len() <= MAX_ROWS as usize, "{screen:?} at {width}");
                let bindings = shown(&rows);
                assert!(bindings.len() <= last, "{screen:?} grew at {width}");
                last = bindings.len();

                let tail: Vec<&str> = bindings.iter().rev().take(2).map(String::as_str).collect();
                assert_eq!(tail, ["q quit", "? keys"], "{screen:?} at {width}");
                if screen != Screen::Weather {
                    assert!(
                        bindings.iter().any(|binding| binding == "esc back"),
                        "{screen:?} at {width}: {bindings:?}"
                    );
                }
                let (own, standing) = bar(&app_on(screen));
                let whole: Vec<String> = own
                    .iter()
                    .chain(&standing)
                    .map(|(key, label)| format!("{key} {label}"))
                    .collect();
                for binding in &bindings {
                    assert!(whole.contains(binding), "{binding:?} was cut at {width}");
                }
            }
        }
    }

    #[test]
    fn the_radar_bar_says_whether_space_pauses_or_plays() {
        let mut app = app_on(Screen::Radar);
        assert!(shown(&legend_at_with(&app, 120)).contains(&"space pause".to_string()));
        app.on_action(Action::TogglePlayback);
        assert!(shown(&legend_at_with(&app, 120)).contains(&"space play".to_string()));
    }

    /// `t` is not on the bar until it is pressed, and then it names the
    /// palette it landed on for a few seconds, on every screen and at the
    /// narrowest width.
    #[test]
    fn the_theme_readout_visits_the_bar_and_leaves() {
        for screen in SCREENS {
            let mut app = app_on(screen);
            assert!(!legend_at_with(&app, 80).concat().contains("theme"));

            app.on_action(Action::CycleTheme);
            let name = app.theme.name();
            for width in [34, 80] {
                let bar = legend_at_with(&app, width).concat();
                assert!(bar.contains(&format!("t theme ({name})")), "{width}: {bar}");
            }

            assert!(app.expire_theme_readout(Instant::now() + Duration::from_secs(60)));
            assert!(!legend_at_with(&app, 80).concat().contains("theme"));
        }
    }

    #[test]
    fn every_palette_can_be_named_on_the_bar() {
        for theme in Theme::ALL {
            let mut app = app_on(Screen::Weather);
            app.theme = theme;
            app.on_action(Action::CycleTheme);
            let name = app.theme.name();
            let bar = legend_at_with(&app, 34).concat();
            assert!(bar.contains(name), "{name} at the minimum width: {bar}");
        }
    }

    #[test]
    fn the_search_bar_names_what_enter_and_escape_do() {
        assert_eq!(
            shown(&legend_at(80, Screen::Search)),
            ["enter search", "esc cancel"]
        );
    }

    /// Left-aligned, the bar hugged the terminal edge while every pane
    /// above it is centred, so it read as detached from the interface.
    #[test]
    fn the_bar_is_centred_and_not_pinned_left() {
        for screen in [
            Screen::Weather,
            Screen::Hourly,
            Screen::Radar,
            Screen::Search,
        ] {
            for row in legend_at(140, screen) {
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

    /// Every key `input` binds on a screen is on that screen's card, and
    /// the card promises no key that does nothing.
    #[test]
    fn the_card_lists_every_key_a_screen_answers_to_and_no_other() {
        use crate::input::action_for;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let code_of = |name: &str| match name {
            "esc" => vec![KeyCode::Esc],
            "space" => vec![KeyCode::Char(' ')],
            "←→↑↓" => vec![KeyCode::Left, KeyCode::Right, KeyCode::Up, KeyCode::Down],
            "←→" => vec![KeyCode::Left, KeyCode::Right],
            "↑↓" => vec![KeyCode::Up, KeyCode::Down],
            one => {
                let mut chars = one.chars();
                match (chars.next(), chars.next()) {
                    (Some(only), None) => vec![KeyCode::Char(only)],
                    _ => panic!("the card names a key this test cannot press: {one:?}"),
                }
            }
        };
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);

        for screen in SCREENS {
            let listed: Vec<KeyCode> = reference(&app_on(screen))
                .iter()
                .flat_map(|group| &group.bindings)
                .flat_map(|(keys, _)| keys.split(' '))
                .flat_map(code_of)
                .collect();
            for code in &listed {
                assert!(
                    action_for(press(*code), screen, false).is_some(),
                    "{screen:?} lists {code:?}, which does nothing"
                );
            }

            let typed = (' '..='~').map(KeyCode::Char);
            let named = [
                KeyCode::Left,
                KeyCode::Right,
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Esc,
                KeyCode::Enter,
                KeyCode::Tab,
                KeyCode::Backspace,
            ];
            // Second spellings of a listed key, which the card leaves out
            // so each job is one line: `=` is `+` unshifted, and `Home`
            // would be `n`. The number of the screen already showing is
            // the one listed key that is meant to do nothing.
            let unlisted = [KeyCode::Char('='), KeyCode::Home];
            for code in typed.chain(named) {
                let bound = action_for(press(code), screen, false).is_some();
                assert!(
                    !bound || listed.contains(&code) || unlisted.contains(&code),
                    "{screen:?} answers to {code:?} and does not list it"
                );
            }
        }
    }
}
