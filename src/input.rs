//! Terminal keys in, domain actions out.
//!
//! This is the only module that knows what a `KeyEvent` is. Everything past it
//! deals in `Action`, which keeps `App` free of Ratatui and Crossterm types and
//! — more usefully — makes the whole of "which key does what" testable without
//! a terminal.

use crate::app::{KeyHintStyle, Screen};
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
    /// Switch the bar between hinting at `?` and naming every binding
    /// itself.
    ToggleKeyHints,
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
            Action::PrevDay
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
/// With the key reference open, "means nothing there" stops existing for
/// presses: the card promises any key closes it, and a key the screen has no
/// binding for would otherwise never reach the app at all — the caller drops
/// a `None` before `App::on_action` can swallow it. Releases and repeats
/// stay filtered as ever; the press that comes with them already closed it.
pub fn action_for(
    key: KeyEvent,
    screen: Screen,
    key_hint_style: KeyHintStyle,
    help_visible: bool,
) -> Option<Action> {
    match key.kind {
        KeyEventKind::Release => return None,
        KeyEventKind::Repeat => {
            let action = binding(key, screen, key_hint_style)?;
            return action.repeatable().then_some(action);
        }
        KeyEventKind::Press => {}
    }
    binding(key, screen, key_hint_style).or_else(|| help_visible.then_some(Action::ToggleHelp))
}

fn binding(key: KeyEvent, screen: Screen, key_hint_style: KeyHintStyle) -> Option<Action> {
    // Checked before the screen bindings so it works even where the plain key
    // means something else — `c` is ordinary text on the search screen.
    // `contains` rather than equality, because terminals do not all report the
    // same modifier set alongside Control.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }

    match screen {
        Screen::Weather => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
            KeyCode::Char('r') => Some(Action::Refresh),
            KeyCode::Char('u') => Some(Action::ToggleUnits),
            KeyCode::Char('t') => Some(Action::CycleTheme),
            KeyCode::Char('l') => Some(Action::OpenSearch),
            KeyCode::Char('p') => Some(Action::OpenHourly),
            KeyCode::Left => Some(Action::PrevDay),
            KeyCode::Right => Some(Action::NextDay),
            // The forecast table lists the days as a column running downward,
            // so the vertical arrows travel it the way it reads: down advances,
            // up goes back — the same convention the hourly screen settled on.
            KeyCode::Up => Some(Action::PrevDay),
            KeyCode::Down => Some(Action::NextDay),
            KeyCode::Char('n') | KeyCode::Home => Some(Action::Today),
            KeyCode::Char('?') => help_key(key_hint_style),
            KeyCode::Char(',') => Some(Action::ToggleKeyHints),
            _ => None,
        },
        Screen::Hourly => match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            // `p` closes the screen as well as opening it, so the key that got
            // you here is always a way back out.
            KeyCode::Char('b' | 'p') | KeyCode::Enter | KeyCode::Esc => Some(Action::Back),
            KeyCode::Char('r') => Some(Action::Refresh),
            KeyCode::Char('u') => Some(Action::ToggleUnits),
            KeyCode::Char('t') => Some(Action::CycleTheme),
            KeyCode::Char('l') => Some(Action::OpenSearch),
            KeyCode::Left => Some(Action::PrevHour),
            KeyCode::Right => Some(Action::NextHour),
            // Down advances and up goes back, as the list convention has it.
            //
            // These used to be the other way round, pairing the vertical arrows
            // with the horizontal ones by direction of travel. That was
            // defensible while the screen had nothing vertical on it to move
            // through, but the week strip draws the days as a literal column
            // running forward from today, and pressing down to travel up it
            // reads as backwards however the reasoning goes.
            KeyCode::Up => Some(Action::PrevHourDay),
            KeyCode::Down => Some(Action::NextHourDay),
            KeyCode::Char('n') | KeyCode::Home => Some(Action::Now),
            KeyCode::Char('v') => Some(Action::ToggleHourlyView),
            KeyCode::Char('?') => help_key(key_hint_style),
            KeyCode::Char(',') => Some(Action::ToggleKeyHints),
            _ => None,
        },
        // Every printable key is text here, so none of the command letters
        // apply — `q` types a q rather than quitting.
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

/// `?` opens the reference only in `Hint` style — in `Full` style the bar
/// already names everything, so there is nothing behind the card to open,
/// and the key is better left unbound than opening an empty one.
fn help_key(key_hint_style: KeyHintStyle) -> Option<Action> {
    (key_hint_style == KeyHintStyle::Hint).then_some(Action::ToggleHelp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shadows the real, four-argument `action_for` for every test below
    /// that does not care which key hint style is active or whether the
    /// reference is open — which is most of them. Tests that do care call
    /// `super::action_for` directly.
    fn action_for(key: KeyEvent, screen: Screen) -> Option<Action> {
        super::action_for(key, screen, KeyHintStyle::Hint, false)
    }

    /// The card's footer promises any key closes it. That has to include
    /// keys the screen has no binding for — Tab, a digit, a function key —
    /// which would otherwise resolve to `None` and never reach the app,
    /// leaving the card sitting there against its own word.
    #[test]
    fn with_the_reference_open_an_unbound_press_still_closes_it() {
        for code in [KeyCode::Tab, KeyCode::Char('5'), KeyCode::F(5)] {
            assert_eq!(
                super::action_for(press(code), Screen::Weather, KeyHintStyle::Hint, true),
                Some(Action::ToggleHelp),
                "{code:?}"
            );
        }
    }

    /// With the reference closed an unbound key stays unbound — the catch-all
    /// exists for the card, not as a new binding.
    #[test]
    fn with_the_reference_closed_an_unbound_key_still_means_nothing() {
        assert_eq!(
            super::action_for(
                press(KeyCode::Tab),
                Screen::Weather,
                KeyHintStyle::Hint,
                false
            ),
            None
        );
    }

    /// Windows reports a release per press, and enhanced protocols repeat.
    /// The press that arrives alongside either has already closed the card,
    /// so acting on them too would toggle it straight back open.
    #[test]
    fn with_the_reference_open_releases_and_repeats_still_do_nothing() {
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            assert_eq!(
                super::action_for(
                    of_kind(KeyCode::Tab, kind),
                    Screen::Weather,
                    KeyHintStyle::Hint,
                    true
                ),
                None,
                "{kind:?}"
            );
        }
    }

    /// `?` opens the key reference wherever a legend hint advertises it, and
    /// stays ordinary text in the search box, where a query might contain one.
    #[test]
    fn question_mark_toggles_help_on_the_weather_screens_only() {
        for screen in [Screen::Weather, Screen::Hourly] {
            assert_eq!(
                action_for(press(KeyCode::Char('?')), screen),
                Some(Action::ToggleHelp),
                "{screen:?}"
            );
        }
        assert_eq!(
            action_for(press(KeyCode::Char('?')), Screen::Search),
            Some(Action::Insert('?'))
        );
    }

    /// `Full` style has no card behind `?` to open, so the key is unbound
    /// there rather than opening an empty one.
    #[test]
    fn question_mark_is_unbound_in_full_style() {
        for screen in [Screen::Weather, Screen::Hourly] {
            assert_eq!(
                super::action_for(press(KeyCode::Char('?')), screen, KeyHintStyle::Full, false),
                None,
                "{screen:?}"
            );
        }
    }

    /// `,` switches the bar style wherever it names a binding at all, and
    /// stays ordinary text in the search box, matching every other command
    /// letter there.
    #[test]
    fn comma_toggles_the_key_hint_style_on_the_weather_screens_only() {
        for screen in [Screen::Weather, Screen::Hourly] {
            assert_eq!(
                action_for(press(KeyCode::Char(',')), screen),
                Some(Action::ToggleKeyHints),
                "{screen:?}"
            );
        }
        assert_eq!(
            action_for(press(KeyCode::Char(',')), Screen::Search),
            Some(Action::Insert(','))
        );
    }

    /// The toggle works the same whichever style is already active — it is
    /// what switches between them, so it cannot be gated by the very thing it
    /// changes.
    #[test]
    fn comma_toggles_from_either_style() {
        for style in [KeyHintStyle::Hint, KeyHintStyle::Full] {
            assert_eq!(
                super::action_for(press(KeyCode::Char(',')), Screen::Weather, style, false),
                Some(Action::ToggleKeyHints),
                "{style:?}"
            );
        }
    }

    /// A toggle on key repeat flickers: a held `?` would open and close the
    /// overlay every frame. Windows also reports a release per press, which
    /// the kind filter already discards — pinned here because a toggle is
    /// where acting twice is most visible.
    #[test]
    fn a_held_or_released_question_mark_does_not_toggle() {
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            assert_eq!(
                action_for(of_kind(KeyCode::Char('?'), kind), Screen::Weather),
                None,
                "{kind:?}"
            );
        }
    }

    /// `v` flips the hourly view, and only there: on the weather screen it is
    /// unbound, and in search it types a letter like any other.
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

    const SCREENS: [Screen; 3] = [Screen::Weather, Screen::Hourly, Screen::Search];

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
            (KeyCode::Char('l'), Screen::Weather),
            (KeyCode::Char('u'), Screen::Weather),
            (KeyCode::Char(','), Screen::Weather),
            (KeyCode::Char(','), Screen::Hourly),
            // Six palettes go past in well under a second on key repeat, and
            // the one you wanted is not the one you land on.
            (KeyCode::Char('t'), Screen::Weather),
            (KeyCode::Char('t'), Screen::Hourly),
            (KeyCode::Char('p'), Screen::Weather),
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
        for c in ['q', 'r', 'u', 't', 'l', 'p', 'b', 'n'] {
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

    /// Esc quits the weather screen but only backs out of the others, so a
    /// stray Esc while searching cannot close the app.
    #[test]
    fn escape_quits_only_from_the_weather_screen() {
        assert_eq!(
            action_for(press(KeyCode::Esc), Screen::Weather),
            Some(Action::Quit)
        );
        assert_eq!(
            action_for(press(KeyCode::Esc), Screen::Hourly),
            Some(Action::Back)
        );
        assert_eq!(
            action_for(press(KeyCode::Esc), Screen::Search),
            Some(Action::Back)
        );
    }

    /// The palette is global, so the key that changes it works from either
    /// screen that shows the weather — the same way `r` and `u` already do.
    #[test]
    fn t_cycles_the_theme_from_both_weather_screens() {
        for screen in [Screen::Weather, Screen::Hourly] {
            assert_eq!(
                action_for(press(KeyCode::Char('t')), screen),
                Some(Action::CycleTheme),
                "{screen:?}"
            );
        }
    }

    #[test]
    fn p_both_opens_and_closes_the_hourly_screen() {
        assert_eq!(
            action_for(press(KeyCode::Char('p')), Screen::Weather),
            Some(Action::OpenHourly)
        );
        assert_eq!(
            action_for(press(KeyCode::Char('p')), Screen::Hourly),
            Some(Action::Back)
        );
    }

    #[test]
    fn unbound_keys_are_ignored_rather_than_guessed_at() {
        for screen in [Screen::Weather, Screen::Hourly] {
            assert_eq!(action_for(press(KeyCode::Tab), screen), None);
            assert_eq!(action_for(press(KeyCode::F(5)), screen), None);
        }
        assert_eq!(action_for(press(KeyCode::Tab), Screen::Search), None);
    }
}
