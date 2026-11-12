//! Terminal keys in, domain actions out.
//!
//! This is the only module that knows what a `KeyEvent` is. Everything past it
//! deals in `Action`, which keeps `App` free of Ratatui and Crossterm types and
//! — more usefully — makes the whole of "which key does what" testable without
//! a terminal.

use crate::app::{MapMove, Screen};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Something the user asked for, named in the app's own terms.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Quit,
    /// Leave the current screen. Where that goes is the app's business.
    Back,
    Refresh,
    ToggleUnits,
    /// Step to the next palette. Global, so it is bound wherever the weather
    /// is on screen.
    CycleTheme,
    OpenSearch,
    OpenHourly,
    OpenRadar,
    /// One frame back or on in the radar loop, which also stops it.
    PrevFrame,
    NextFrame,
    /// Stop or restart the radar loop.
    TogglePlayback,
    /// Zoom or pan the radar's map.
    MoveMap(MapMove),
    PrevDay,
    NextDay,
    Today,
    PrevHour,
    NextHour,
    PrevHourDay,
    NextHourDay,
    Now,
    /// Flip the hourly screen between the weathergram and the classic
    /// precipitation view.
    ToggleHourlyView,
    /// Open or close the key reference overlay.
    ToggleHelp,
    Insert(char),
    Backspace,
    Submit,
    PrevResult,
    NextResult,
}

impl Action {
    /// Whether holding the key down may fire this action repeatedly.
    ///
    /// Moving around and editing text are what people actually hold a key for.
    /// Everything else either sends a network request or changes screen, and a
    /// held key doing that repeatedly is how an unbounded queue of requests
    /// gets built by accident.
    fn repeatable(self) -> bool {
        matches!(
            self,
            Action::PrevFrame
                | Action::NextFrame
                // Held, these pass through a view a repeat, and none of
                // them is asked for until the key comes up: the app waits
                // for the map to stop.
                | Action::MoveMap(MapMove::In | MapMove::Out | MapMove::Pan(..))
                | Action::PrevDay
                | Action::NextDay
                | Action::PrevHour
                | Action::NextHour
                | Action::PrevHourDay
                | Action::NextHourDay
                | Action::PrevResult
                | Action::NextResult
                | Action::Insert(_)
                | Action::Backspace
        )
    }
}

/// Which keys are down right now, for terminals that say when one comes up.
///
/// Crossterm's Windows backend never reports `Repeat`. A held key arrives as
/// a stream of `Press` events, each indistinguishable from a fresh keystroke,
/// so the repeat filter in `action_for` did nothing there and a held `t`
/// walked through every theme. What Windows does report is a `Release` for
/// every key, and that is enough: a second `Press` with no release in between
/// is the repeat, and gets relabelled as one before the filter sees it.
///
/// Legacy Unix terminals report neither, and a held set that nothing ever
/// empties would call every second keystroke a repeat. So the tracking is
/// armed on Windows and nowhere else.
///
/// An earlier draft armed it from the first release seen anywhere, on the
/// theory that one release proves the terminal reports them. It does not.
/// The kitty protocol reports releases for the keys that produce text but
/// not for Enter, Tab, or Backspace unless "report all keys as escape codes"
/// is also negotiated, so an arrow's release followed by two Enters silently
/// turned the second Enter into a repeat and every Enter after it too. A
/// terminal speaking that protocol labels its own repeats, so nothing on
/// Unix needs the synthetic relabel in the first place.
#[derive(Debug, Default)]
pub struct HeldKeys {
    armed: bool,
    down: Vec<KeyCode>,
}

impl HeldKeys {
    /// `reports_releases` is passed rather than read from `cfg!` so both
    /// arms are testable everywhere; the caller passes `cfg!(windows)`.
    pub fn new(reports_releases: bool) -> Self {
        Self {
            armed: reports_releases,
            down: Vec::new(),
        }
    }

    /// The same event, with a press that is really a repeat labelled as one.
    pub fn observe(&mut self, mut key: KeyEvent) -> KeyEvent {
        match key.kind {
            KeyEventKind::Release => {
                self.down.retain(|code| *code != key.code);
            }
            KeyEventKind::Press if self.armed => {
                if self.down.contains(&key.code) {
                    key.kind = KeyEventKind::Repeat;
                } else {
                    self.down.push(key.code);
                }
            }
            KeyEventKind::Press | KeyEventKind::Repeat => {}
        }
        key
    }
}

/// The action a key means on `screen`, or `None` if it means nothing there.
///
/// Event *kind* is filtered here rather than deeper in: Crossterm's Windows
/// backend reports press and release for every keystroke, and enhanced
/// terminal protocols can add repeats on Unix too. Acting on a release would
/// double every keystroke and every request. A Windows repeat, which arrives
/// as another press, is relabelled by `HeldKeys` before it gets here.
///
/// With the key reference open only the keys that close it mean anything.
/// A key that acted on the screen behind the card would be acting on
/// something the reader cannot see, and a card that any stray key closes
/// is gone before it has been read.
pub fn action_for(key: KeyEvent, screen: Screen, help_visible: bool) -> Option<Action> {
    let action = if help_visible {
        help_binding(key)
    } else {
        binding(key, screen)
    }?;
    match key.kind {
        KeyEventKind::Release => None,
        KeyEventKind::Repeat => action.repeatable().then_some(action),
        KeyEventKind::Press => Some(action),
    }
}

/// Checked before any screen's bindings so it works even where the plain
/// key means something else: `c` is ordinary text on the search screen.
/// `contains` and not equality, because terminals do not all report the same
/// modifier set alongside Control.
fn is_interrupt(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

fn help_binding(key: KeyEvent) -> Option<Action> {
    if is_interrupt(key) {
        return Some(Action::Quit);
    }
    match key.code {
        KeyCode::Char('?') | KeyCode::Esc => Some(Action::ToggleHelp),
        _ => None,
    }
}

fn binding(key: KeyEvent, screen: Screen) -> Option<Action> {
    if is_interrupt(key) {
        return Some(Action::Quit);
    }
    match screen {
        Screen::Weather => match key.code {
            KeyCode::Left | KeyCode::Up => Some(Action::PrevDay),
            // The forecast table lists the days as a column running
            // downward, so the vertical arrows travel it the way it reads.
            KeyCode::Right | KeyCode::Down => Some(Action::NextDay),
            KeyCode::Char('n') | KeyCode::Home => Some(Action::Today),
            _ => shared(key.code, screen),
        },
        Screen::Hourly => match key.code {
            KeyCode::Left => Some(Action::PrevHour),
            KeyCode::Right => Some(Action::NextHour),
            // Down advances and up goes back, as a list has it: the week
            // strip draws the days as a column running forward from today.
            KeyCode::Up => Some(Action::PrevHourDay),
            KeyCode::Down => Some(Action::NextHourDay),
            KeyCode::Char('n') | KeyCode::Home => Some(Action::Now),
            KeyCode::Char('v') => Some(Action::ToggleHourlyView),
            _ => shared(key.code, screen),
        },
        Screen::Radar => match key.code {
            KeyCode::Left => Some(Action::MoveMap(MapMove::Pan(-1, 0))),
            KeyCode::Right => Some(Action::MoveMap(MapMove::Pan(1, 0))),
            KeyCode::Up => Some(Action::MoveMap(MapMove::Pan(0, -1))),
            KeyCode::Down => Some(Action::MoveMap(MapMove::Pan(0, 1))),
            // `=` is `+` without the shift key, on the layouts where `+`
            // needs one.
            KeyCode::Char('+' | '=') => Some(Action::MoveMap(MapMove::In)),
            KeyCode::Char('-') => Some(Action::MoveMap(MapMove::Out)),
            KeyCode::Char('n') | KeyCode::Home => Some(Action::MoveMap(MapMove::Home)),
            // The keys `<` and `>` are printed on, without the shift.
            KeyCode::Char(',') => Some(Action::PrevFrame),
            KeyCode::Char('.') => Some(Action::NextFrame),
            KeyCode::Char(' ') => Some(Action::TogglePlayback),
            _ => shared(key.code, screen),
        },
        // Every printable key is text here, so none of the command letters
        // apply: `q` types a q and does not quit.
        Screen::Search => match key.code {
            KeyCode::Esc => Some(Action::Back),
            KeyCode::Enter => Some(Action::Submit),
            KeyCode::Backspace => Some(Action::Backspace),
            // A chord is not text; `is_chord` says which modifiers make one.
            KeyCode::Char(_) if is_chord(key.modifiers) => None,
            KeyCode::Char(c) => Some(Action::Insert(c)),
            KeyCode::Up => Some(Action::PrevResult),
            KeyCode::Down => Some(Action::NextResult),
            _ => None,
        },
    }
}

/// The keys that mean the same on the forecast, the hourly screen and the
/// radar. A key learned on one is not relearned on the next.
///
/// A screen's number goes to it and its letter goes to it or, from it, back:
/// the key that got you somewhere is always a way out again. The number of
/// the screen already showing does nothing, so `1` `2` `3` can be pressed
/// without first working out where one is.
fn shared(code: KeyCode, screen: Screen) -> Option<Action> {
    let on = |here: Screen| screen == here;
    match code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('/') => Some(Action::OpenSearch),
        KeyCode::Char('r') => Some(Action::Refresh),
        KeyCode::Char('u') => Some(Action::ToggleUnits),
        KeyCode::Char('t') => Some(Action::CycleTheme),
        KeyCode::Char('1') => (!on(Screen::Weather)).then_some(Action::Back),
        KeyCode::Char('2') => (!on(Screen::Hourly)).then_some(Action::OpenHourly),
        KeyCode::Char('3') => (!on(Screen::Radar)).then_some(Action::OpenRadar),
        KeyCode::Char('h') if on(Screen::Hourly) => Some(Action::Back),
        KeyCode::Char('h') => Some(Action::OpenHourly),
        KeyCode::Char('m') if on(Screen::Radar) => Some(Action::Back),
        KeyCode::Char('m') => Some(Action::OpenRadar),
        // Never quits. From the forecast there is nowhere back to go, and a
        // key that closes things on two screens and the app on the third
        // gets pressed once too often.
        KeyCode::Esc => (!on(Screen::Weather)).then_some(Action::Back),
        _ => None,
    }
}

/// Control or Alt alone makes a letter a chord: Ctrl-U, Ctrl-W and
/// Alt-anything are the line-editing keys people reach for by habit, and
/// inserting a literal `u` or `w` for them helps nobody. Shift is not a
/// chord; it is how a capital letter arrives.
///
/// Both together is typed, because on Windows that pair is AltGr: the
/// console reports a third-level glyph — `ł`, `€`, `@` on a Polish or
/// German layout — as left Control plus right Alt, and crossterm passes it
/// on as `CONTROL | ALT` with the glyph already in the char. Refusing it
/// would make "Łódź" untypeable on the one screen whose job is typing a
/// city. On Unix the same pair is a real Ctrl-Alt-letter chord, and it is
/// typed anyway: nobody's line-editing habit binds one, so that is the
/// cheaper side to be wrong on.
fn is_chord(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL) != modifiers.contains(KeyModifiers::ALT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEATHER_SCREENS: [Screen; 3] = [Screen::Weather, Screen::Hourly, Screen::Radar];

    /// Shadows the real `action_for` for every test below that does not
    /// care whether the reference is open, which is most of them. Tests
    /// that do care call `super::action_for` directly.
    fn action_for(key: KeyEvent, screen: Screen) -> Option<Action> {
        super::action_for(key, screen, false)
    }

    /// A screen's number goes to it from either of the others and does
    /// nothing on it. Its letter goes to it, and from it goes back.
    #[test]
    fn each_screen_has_a_number_and_a_letter() {
        let key = |c: char, screen| action_for(press(KeyCode::Char(c)), screen);
        let table = [
            (Screen::Weather, '1', None, Action::Back),
            (Screen::Hourly, '2', Some('h'), Action::OpenHourly),
            (Screen::Radar, '3', Some('m'), Action::OpenRadar),
        ];
        for (target, number, letter, action) in table {
            for from in WEATHER_SCREENS {
                let there = from == target;
                assert_eq!(
                    key(number, from),
                    (!there).then_some(action),
                    "{number} on {from:?}"
                );
                if let Some(letter) = letter {
                    let wanted = if there { Action::Back } else { action };
                    assert_eq!(key(letter, from), Some(wanted), "{letter} on {from:?}");
                }
            }
        }
    }

    /// The keys that mean the same thing wherever the weather is on
    /// screen, the radar included.
    #[test]
    fn the_shared_keys_work_on_every_weather_screen() {
        let shared = [
            ('q', Action::Quit),
            ('r', Action::Refresh),
            ('u', Action::ToggleUnits),
            ('t', Action::CycleTheme),
            ('/', Action::OpenSearch),
            ('?', Action::ToggleHelp),
        ];
        for screen in WEATHER_SCREENS {
            for (c, action) in shared {
                assert_eq!(
                    action_for(press(KeyCode::Char(c)), screen),
                    Some(action),
                    "{c} on {screen:?}"
                );
            }
        }
    }

    /// One key for "back to here and now", whatever that is on the screen:
    /// today, this hour, or the city at the opening zoom.
    #[test]
    fn n_and_home_come_back_to_here_and_now() {
        let table = [
            (Screen::Weather, Action::Today),
            (Screen::Hourly, Action::Now),
            (Screen::Radar, Action::MoveMap(MapMove::Home)),
        ];
        for (screen, action) in table {
            for code in [KeyCode::Char('n'), KeyCode::Home] {
                assert_eq!(action_for(press(code), screen), Some(action), "{screen:?}");
            }
        }
    }

    /// Escape goes back and never quits: from the forecast there is nowhere
    /// back to go, so there it does nothing.
    #[test]
    fn escape_goes_back_and_never_quits() {
        assert_eq!(action_for(press(KeyCode::Esc), Screen::Weather), None);
        for screen in [Screen::Hourly, Screen::Radar, Screen::Search] {
            assert_eq!(
                action_for(press(KeyCode::Esc), screen),
                Some(Action::Back),
                "{screen:?}"
            );
        }
    }

    /// The keys this scheme took away do nothing, so an old habit fails
    /// quietly and does not do something else.
    #[test]
    fn the_retired_keys_are_unbound() {
        let retired = [
            (Screen::Weather, KeyCode::Char('p')),
            (Screen::Weather, KeyCode::Char('l')),
            (Screen::Weather, KeyCode::Char(',')),
            (Screen::Hourly, KeyCode::Char('p')),
            (Screen::Hourly, KeyCode::Char('b')),
            (Screen::Hourly, KeyCode::Char(',')),
            (Screen::Hourly, KeyCode::Enter),
            (Screen::Radar, KeyCode::Char('b')),
            (Screen::Radar, KeyCode::Char('c')),
            (Screen::Radar, KeyCode::Char('[')),
            (Screen::Radar, KeyCode::Char(']')),
            (Screen::Radar, KeyCode::Enter),
        ];
        for (screen, code) in retired {
            assert_eq!(
                action_for(press(code), screen),
                None,
                "{code:?} on {screen:?}"
            );
        }
    }

    /// Comma and full stop scrub and may be held; space and `r` act once a
    /// press, since a held `r` would queue a radar fetch a repeat.
    #[test]
    fn the_radar_loop_is_scrubbed_with_comma_and_stop_and_paused_with_space() {
        assert_eq!(
            action_for(press(KeyCode::Char(',')), Screen::Radar),
            Some(Action::PrevFrame)
        );
        assert_eq!(
            action_for(press(KeyCode::Char('.')), Screen::Radar),
            Some(Action::NextFrame)
        );
        assert_eq!(
            action_for(press(KeyCode::Char(' ')), Screen::Radar),
            Some(Action::TogglePlayback)
        );
        assert_eq!(
            action_for(
                of_kind(KeyCode::Char('.'), KeyEventKind::Repeat),
                Screen::Radar
            ),
            Some(Action::NextFrame)
        );
        for code in [KeyCode::Char(' '), KeyCode::Char('r')] {
            assert_eq!(
                action_for(of_kind(code, KeyEventKind::Repeat), Screen::Radar),
                None,
                "{code:?}"
            );
        }
    }

    /// The arrows pan and `+` and `-` zoom, held or not. Going home is once
    /// a press.
    #[test]
    fn the_radar_map_is_moved_with_the_arrows_and_zoomed_with_plus_and_minus() {
        let moves = [
            (KeyCode::Left, MapMove::Pan(-1, 0)),
            (KeyCode::Right, MapMove::Pan(1, 0)),
            (KeyCode::Up, MapMove::Pan(0, -1)),
            (KeyCode::Down, MapMove::Pan(0, 1)),
            (KeyCode::Char('+'), MapMove::In),
            (KeyCode::Char('='), MapMove::In),
            (KeyCode::Char('-'), MapMove::Out),
        ];
        for (code, to) in moves {
            for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
                assert_eq!(
                    action_for(of_kind(code, kind), Screen::Radar),
                    Some(Action::MoveMap(to)),
                    "{code:?}"
                );
            }
        }
        assert_eq!(
            action_for(
                of_kind(KeyCode::Char('n'), KeyEventKind::Repeat),
                Screen::Radar
            ),
            None
        );
    }

    /// With the reference open, `?` and Escape close it, Ctrl-C still
    /// quits, and every other key does nothing at all: not its usual job,
    /// and not closing the card either.
    #[test]
    fn with_the_reference_open_only_the_closing_keys_mean_anything() {
        for screen in WEATHER_SCREENS {
            for code in [KeyCode::Char('?'), KeyCode::Esc] {
                assert_eq!(
                    super::action_for(press(code), screen, true),
                    Some(Action::ToggleHelp),
                    "{code:?} on {screen:?}"
                );
            }
            for code in [
                KeyCode::Char('q'),
                KeyCode::Char('r'),
                KeyCode::Char('x'),
                KeyCode::Char(' '),
                KeyCode::Left,
                KeyCode::Enter,
                KeyCode::F(5),
            ] {
                assert_eq!(
                    super::action_for(press(code), screen, true),
                    None,
                    "{code:?} on {screen:?}"
                );
            }
            let interrupt = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
            assert_eq!(
                super::action_for(interrupt, screen, true),
                Some(Action::Quit)
            );
        }
    }

    /// A release never acts and a held `?` does not flicker the card open
    /// and shut, with the card up or down.
    #[test]
    fn a_held_or_released_question_mark_does_not_toggle() {
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            for open in [false, true] {
                assert_eq!(
                    super::action_for(of_kind(KeyCode::Char('?'), kind), Screen::Weather, open),
                    None,
                    "{kind:?}, open: {open}"
                );
            }
        }
    }

    /// `?` is text where text is being typed.
    #[test]
    fn question_mark_is_text_on_the_search_screen() {
        assert_eq!(
            action_for(press(KeyCode::Char('?')), Screen::Search),
            Some(Action::Insert('?'))
        );
    }

    #[test]
    fn v_toggles_the_hourly_view_only_on_the_hourly_screen() {
        assert_eq!(
            action_for(press(KeyCode::Char('v')), Screen::Hourly),
            Some(Action::ToggleHourlyView)
        );
        assert_eq!(action_for(press(KeyCode::Char('v')), Screen::Weather), None);
        assert_eq!(
            action_for(press(KeyCode::Char('v')), Screen::Search),
            Some(Action::Insert('v'))
        );
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn of_kind(code: KeyCode, kind: KeyEventKind) -> KeyEvent {
        let mut key = press(code);
        key.kind = kind;
        key
    }

    const SCREENS: [Screen; 4] = [
        Screen::Weather,
        Screen::Hourly,
        Screen::Radar,
        Screen::Search,
    ];

    /// The Windows backend reports a release for every press. Acting on both
    /// types every character twice and fires every action twice.
    #[test]
    fn a_key_release_never_means_anything() {
        for screen in SCREENS {
            for code in [
                KeyCode::Char('r'),
                KeyCode::Char('q'),
                KeyCode::Char('a'),
                KeyCode::Left,
                KeyCode::Enter,
                KeyCode::Esc,
                KeyCode::Backspace,
            ] {
                assert_eq!(
                    action_for(of_kind(code, KeyEventKind::Release), screen),
                    None,
                    "{code:?} on {screen:?} acted on release"
                );
            }
        }
    }

    /// A Windows-style press/release pair must insert exactly one character.
    #[test]
    fn a_press_and_release_pair_types_one_character() {
        let typed: Vec<Action> = [KeyEventKind::Press, KeyEventKind::Release]
            .into_iter()
            .filter_map(|kind| action_for(of_kind(KeyCode::Char('x'), kind), Screen::Search))
            .collect();

        assert_eq!(typed, vec![Action::Insert('x')]);
    }

    /// Holding an arrow should scroll. Holding `r` should not queue a fetch per
    /// frame, which is how the request channel grows without bound.
    #[test]
    fn only_movement_and_typing_repeat_when_a_key_is_held() {
        let repeat = |code, screen| action_for(of_kind(code, KeyEventKind::Repeat), screen);

        assert_eq!(
            repeat(KeyCode::Left, Screen::Weather),
            Some(Action::PrevDay)
        );
        assert_eq!(
            repeat(KeyCode::Up, Screen::Hourly),
            Some(Action::PrevHourDay)
        );
        assert_eq!(
            repeat(KeyCode::Char('a'), Screen::Search),
            Some(Action::Insert('a'))
        );
        assert_eq!(
            repeat(KeyCode::Backspace, Screen::Search),
            Some(Action::Backspace)
        );

        for (code, screen) in [
            (KeyCode::Char('r'), Screen::Weather),
            (KeyCode::Char('/'), Screen::Weather),
            (KeyCode::Char('u'), Screen::Weather),
            (KeyCode::Char('?'), Screen::Hourly),
            // Six palettes go past in well under a second on key repeat, and
            // the one you wanted is not the one you land on.
            (KeyCode::Char('t'), Screen::Weather),
            (KeyCode::Char('t'), Screen::Hourly),
            (KeyCode::Char('h'), Screen::Weather),
            (KeyCode::Char('3'), Screen::Weather),
            (KeyCode::Char('q'), Screen::Weather),
            (KeyCode::Char('r'), Screen::Hourly),
            (KeyCode::Enter, Screen::Search),
            (KeyCode::Esc, Screen::Search),
        ] {
            assert_eq!(
                repeat(code, screen),
                None,
                "held {code:?} repeated on {screen:?}"
            );
        }
    }

    /// What Windows actually sends for a held `t`: press, press, press, and
    /// a single release at the end. Only the first press may act.
    #[test]
    fn a_press_with_no_release_between_is_a_repeat_where_releases_are_reported() {
        let mut held = HeldKeys::new(true);
        let actions: Vec<Option<Action>> = [
            KeyEventKind::Press,
            KeyEventKind::Press,
            KeyEventKind::Press,
            KeyEventKind::Release,
        ]
        .into_iter()
        .map(|kind| {
            action_for(
                held.observe(of_kind(KeyCode::Char('t'), kind)),
                Screen::Weather,
            )
        })
        .collect();

        assert_eq!(
            actions,
            [Some(Action::CycleTheme), None, None, None],
            "a held t cycled the theme more than once"
        );
    }

    /// The relabelling must not cost the keys that are meant to repeat: a
    /// held arrow on Windows still scrolls.
    #[test]
    fn a_held_arrow_still_repeats_where_releases_are_reported() {
        let mut held = HeldKeys::new(true);
        let actions: Vec<Option<Action>> = [KeyEventKind::Press; 3]
            .into_iter()
            .map(|kind| action_for(held.observe(of_kind(KeyCode::Left, kind)), Screen::Weather))
            .collect();

        assert_eq!(actions, [Some(Action::PrevDay); 3]);
    }

    /// Two distinct keystrokes are two: a release between the presses ends
    /// the hold, and the next press is a fresh one.
    #[test]
    fn a_release_ends_the_hold() {
        let mut held = HeldKeys::new(true);
        let mut press = |kind| {
            action_for(
                held.observe(of_kind(KeyCode::Char('u'), kind)),
                Screen::Weather,
            )
        };

        assert_eq!(press(KeyEventKind::Press), Some(Action::ToggleUnits));
        assert_eq!(press(KeyEventKind::Release), None);
        assert_eq!(press(KeyEventKind::Press), Some(Action::ToggleUnits));
    }

    /// Holding one key must not make a different key look held.
    #[test]
    fn holds_are_tracked_per_key() {
        let mut held = HeldKeys::new(true);
        assert_eq!(
            action_for(held.observe(press(KeyCode::Char('t'))), Screen::Weather),
            Some(Action::CycleTheme)
        );
        assert_eq!(
            action_for(held.observe(press(KeyCode::Char('u'))), Screen::Weather),
            Some(Action::ToggleUnits)
        );
    }

    /// A legacy Unix terminal never reports a release, so consecutive presses
    /// of the same key are consecutive keystrokes and every one must act.
    #[test]
    fn without_releases_every_press_is_a_keystroke() {
        let mut held = HeldKeys::new(false);
        let actions: Vec<Option<Action>> = [KeyEventKind::Press; 3]
            .into_iter()
            .map(|kind| {
                action_for(
                    held.observe(of_kind(KeyCode::Char('t'), kind)),
                    Screen::Weather,
                )
            })
            .collect();

        assert_eq!(actions, [Some(Action::CycleTheme); 3]);
    }

    /// A release seen on a terminal that was not declared to report them
    /// proves nothing about the other keys, so it must not arm the tracking.
    #[test]
    fn a_release_alone_does_not_arm_the_tracking() {
        let mut held = HeldKeys::new(false);
        let mut observe =
            |code, kind| action_for(held.observe(of_kind(code, kind)), Screen::Weather);

        assert_eq!(observe(KeyCode::Char('x'), KeyEventKind::Press), None);
        assert_eq!(observe(KeyCode::Char('x'), KeyEventKind::Release), None);

        for _ in 0..2 {
            assert_eq!(
                observe(KeyCode::Char('t'), KeyEventKind::Press),
                Some(Action::CycleTheme),
                "a repeated press was suppressed after a lone release"
            );
        }
    }

    /// The kitty protocol releases an arrow but never Enter, Tab, or
    /// Backspace unless every key is reported as an escape code. Armed by
    /// that arrow's release, the tracking called the second Enter a repeat
    /// and dropped the submit, and every submit after it.
    #[test]
    fn an_arrow_release_does_not_make_the_next_enter_a_repeat() {
        let mut held = HeldKeys::new(false);
        let mut observe =
            |code, kind| action_for(held.observe(of_kind(code, kind)), Screen::Search);

        assert_eq!(
            observe(KeyCode::Left, KeyEventKind::Press),
            None,
            "left is unbound on the search screen"
        );
        assert_eq!(observe(KeyCode::Left, KeyEventKind::Release), None);

        assert_eq!(
            observe(KeyCode::Enter, KeyEventKind::Press),
            Some(Action::Submit)
        );
        assert_eq!(
            observe(KeyCode::Enter, KeyEventKind::Press),
            Some(Action::Submit),
            "a second Enter with no release between was dropped as a repeat"
        );
    }

    /// The kitty protocol labels its repeats itself; passing them through
    /// unchanged keeps that path exactly as it was.
    #[test]
    fn a_labelled_repeat_passes_through_untouched() {
        let mut held = HeldKeys::new(true);
        let key = held.observe(of_kind(KeyCode::Left, KeyEventKind::Repeat));
        assert_eq!(key.kind, KeyEventKind::Repeat);
        assert_eq!(action_for(key, Screen::Weather), Some(Action::PrevDay));
    }

    /// Ctrl-C quits from anywhere, including where `c` is ordinary text.
    #[test]
    fn ctrl_c_quits_from_every_screen() {
        for screen in SCREENS {
            let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
            assert_eq!(action_for(key, screen), Some(Action::Quit), "{screen:?}");
        }
    }

    /// Terminals do not all report the same modifier set, so an exact match on
    /// CONTROL would miss Ctrl-Shift-C and friends.
    #[test]
    fn ctrl_c_survives_extra_modifiers() {
        for extra in [
            KeyModifiers::SHIFT,
            KeyModifiers::ALT,
            KeyModifiers::SHIFT | KeyModifiers::ALT,
        ] {
            let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL | extra);
            assert_eq!(
                action_for(key, Screen::Weather),
                Some(Action::Quit),
                "{extra:?}"
            );
        }
    }

    /// Ctrl-U, Ctrl-W, Ctrl-A and Alt-anything are line-editing chords by
    /// habit. Typing the bare letter for them is wrong in every terminal, so
    /// they mean nothing here until they mean something.
    #[test]
    fn control_and_alt_letters_are_not_typed_into_the_search() {
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        ] {
            for c in ['u', 'w', 'a', 'x'] {
                let key = KeyEvent::new(KeyCode::Char(c), modifiers);
                assert_eq!(
                    action_for(key, Screen::Search),
                    None,
                    "{modifiers:?}+{c} was typed"
                );
            }
        }
    }

    /// The Windows console reports an AltGr glyph as Control plus Alt with
    /// the translated character already in the event, so that pairing is
    /// text, not a chord: the accented and currency characters of every
    /// European layout arrive this way and nothing else can type them.
    #[test]
    fn an_altgr_glyph_is_still_text_on_the_search_screen() {
        let altgr = KeyModifiers::CONTROL | KeyModifiers::ALT;
        for c in ['\u{142}', '\u{20ac}', '@'] {
            let key = KeyEvent::new(KeyCode::Char(c), altgr);
            assert_eq!(
                action_for(key, Screen::Search),
                Some(Action::Insert(c)),
                "{c}"
            );
        }
        // A plain letter under the same pair is a real chord on Unix and is
        // typed anyway; a "fix" for that would take Windows AltGr with it.
        let key = KeyEvent::new(KeyCode::Char('u'), altgr);
        assert_eq!(action_for(key, Screen::Search), Some(Action::Insert('u')));
        // The capital in "Łódź" is AltGr with Shift held, which the console
        // reports as all three; an equality test on the pair would drop it.
        let key = KeyEvent::new(KeyCode::Char('\u{141}'), altgr | KeyModifiers::SHIFT);
        assert_eq!(
            action_for(key, Screen::Search),
            Some(Action::Insert('\u{141}'))
        );
    }

    /// Shift is how a capital letter arrives, not a chord, so it must keep
    /// typing — "New York" needs both.
    #[test]
    fn a_shifted_letter_is_still_text_on_the_search_screen() {
        let key = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT);
        assert_eq!(action_for(key, Screen::Search), Some(Action::Insert('N')));
    }

    /// Plain `c` on the search screen is a letter, not a quit.
    #[test]
    fn an_unmodified_c_is_text_on_the_search_screen() {
        assert_eq!(
            action_for(press(KeyCode::Char('c')), Screen::Search),
            Some(Action::Insert('c'))
        );
    }

    /// Every command letter is text once the search box has focus, or typing a
    /// city with a `q` or an `r` in it would set the app off.
    #[test]
    fn command_letters_are_text_on_the_search_screen() {
        for c in [
            'q', 'r', 'u', 't', 'h', 'm', 'n', '1', '2', '3', '/', ',', '.',
        ] {
            assert_eq!(
                action_for(press(KeyCode::Char(c)), Screen::Search),
                Some(Action::Insert(c)),
                "{c:?} was treated as a command"
            );
        }
    }

    #[test]
    fn the_arrows_mean_different_things_on_each_screen() {
        assert_eq!(
            action_for(press(KeyCode::Left), Screen::Weather),
            Some(Action::PrevDay)
        );
        assert_eq!(
            action_for(press(KeyCode::Left), Screen::Hourly),
            Some(Action::PrevHour)
        );
        assert_eq!(
            action_for(press(KeyCode::Down), Screen::Search),
            Some(Action::NextResult)
        );
    }

    /// The forecast table lists the days as a column, so the vertical arrows
    /// traverse them too: down advances and up goes back, matching both the
    /// table's reading order and the hourly screen's convention.
    #[test]
    fn the_vertical_arrows_traverse_days_on_the_weather_screen() {
        assert_eq!(
            action_for(press(KeyCode::Up), Screen::Weather),
            Some(Action::PrevDay)
        );
        assert_eq!(
            action_for(press(KeyCode::Down), Screen::Weather),
            Some(Action::NextDay)
        );
        // Held arrows scroll here the same as the horizontal pair.
        assert_eq!(
            action_for(
                of_kind(KeyCode::Down, KeyEventKind::Repeat),
                Screen::Weather
            ),
            Some(Action::NextDay)
        );
    }

    /// Down advances through time and up goes back, matching the week strip,
    /// which draws the days as a column running forward from today.
    #[test]
    fn the_day_arrows_point_the_way_the_week_strip_reads() {
        assert_eq!(
            action_for(press(KeyCode::Up), Screen::Hourly),
            Some(Action::PrevHourDay)
        );
        assert_eq!(
            action_for(press(KeyCode::Down), Screen::Hourly),
            Some(Action::NextHourDay)
        );
    }

    #[test]
    fn unbound_keys_are_ignored_rather_than_guessed_at() {
        for screen in WEATHER_SCREENS {
            assert_eq!(action_for(press(KeyCode::Tab), screen), None);
            assert_eq!(action_for(press(KeyCode::F(5)), screen), None);
        }
        assert_eq!(action_for(press(KeyCode::Tab), Screen::Search), None);
    }
}
