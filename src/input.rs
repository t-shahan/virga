//! Terminal keys in, domain actions out.
//!
//! This is the only module that knows what a `KeyEvent` is. Everything past it
//! deals in `Action`, which keeps `App` free of Ratatui and Crossterm types and
//! — more usefully — makes the whole of "which key does what" testable without
//! a terminal.

use crate::app::Screen;
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

/// The action a key means on `screen`, or `None` if it means nothing there.
///
/// Event *kind* is filtered here rather than deeper in: Crossterm's Windows
/// backend reports press and release for every keystroke, and enhanced
/// terminal protocols can add repeats on Unix too. Acting on a release would
/// double every keystroke and every request.
pub fn action_for(key: KeyEvent, screen: Screen) -> Option<Action> {
    match key.kind {
        KeyEventKind::Release => return None,
        KeyEventKind::Repeat => {
            let action = binding(key, screen)?;
            return action.repeatable().then_some(action);
        }
        KeyEventKind::Press => {}
    }
    binding(key, screen)
}

fn binding(key: KeyEvent, screen: Screen) -> Option<Action> {
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
            KeyCode::Char('?') => Some(Action::ToggleHelp),
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
            KeyCode::Char('?') => Some(Action::ToggleHelp),
            _ => None,
        },
        // Every printable key is text here, so none of the command letters
        // apply — `q` types a q rather than quitting.
        Screen::Search => match key.code {
            KeyCode::Esc => Some(Action::Back),
            KeyCode::Enter => Some(Action::Submit),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char(c) => Some(Action::Insert(c)),
            KeyCode::Up => Some(Action::PrevResult),
            KeyCode::Down => Some(Action::NextResult),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
