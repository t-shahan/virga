//! What the colours mean, and what each theme makes of them.
//!
//! Widgets ask for a *role* — the selection, the accent, a muted label — and
//! never for a colour. That indirection is the whole feature: adding a theme
//! is filling in one more [`Palette`], and no widget has to be touched or
//! even know that themes exist.
//!
//! `Theme` is a name and nothing else, so `App` can hold the setting without
//! taking on a Ratatui type. Only `ui` ever resolves one to a `Palette`.

use ratatui::style::Color;

/// The nine meanings the interface actually has: one ground and eight things
/// drawn on it.
///
/// `accent` and `series` are the same blue in the terminal palette but are
/// separate roles: one is the app's voice — the city, the temperature — and
/// the other is data. A theme is free to split them, and most do.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    /// The ground the whole frame is painted on, before anything is drawn on
    /// top. Every named palette here was designed against a dark terminal, so
    /// leaving this to the terminal put light-scheme users in front of text at
    /// barely over 1:1 — the palette has to bring its own background or it is
    /// only correct by luck. `Reset` hands the ground back to the terminal.
    pub background: Color,
    /// City name, hero digits, the search prompt.
    pub accent: Color,
    /// Readings: the values the app went to the network for.
    pub text: Color,
    /// Labels, headers, and the notes hung off a border.
    pub muted: Color,
    /// Whatever the arrows are currently on.
    pub selection: Color,
    /// Today, or the current hour — a fixed reference the selection moves over.
    pub now: Color,
    /// Ordinary bars and columns: neither selected nor now.
    pub series: Color,
    /// A failure the user has to read.
    pub error: Color,
    /// Box edges. `Reset` leaves them the terminal's own colour.
    pub border: Color,
}

/// A named palette. Cycled with `t`, defaulting to the terminal's own colours.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Theme {
    /// The sixteen ANSI colours, so the app looks the way the terminal was
    /// configured to look. The default, and the only palette that renders
    /// correctly without 24-bit colour.
    #[default]
    Terminal,
    CatppuccinMocha,
    GruvboxDark,
    Nord,
    TokyoNight,
    Dracula,
}

impl Theme {
    /// Every theme, in cycle order. Kept honest by
    /// `cycling_visits_every_theme_and_comes_back`: `next` is the definition,
    /// and this array has to agree with it.
    pub const ALL: [Theme; 6] = [
        Theme::Terminal,
        Theme::CatppuccinMocha,
        Theme::GruvboxDark,
        Theme::Nord,
        Theme::TokyoNight,
        Theme::Dracula,
    ];

    /// The next theme along, wrapping.
    ///
    /// An exhaustive match rather than an index into `ALL`, so adding a
    /// variant without deciding where it sits in the cycle fails the build
    /// instead of quietly stranding it.
    pub fn next(self) -> Self {
        match self {
            Theme::Terminal => Theme::CatppuccinMocha,
            Theme::CatppuccinMocha => Theme::GruvboxDark,
            Theme::GruvboxDark => Theme::Nord,
            Theme::Nord => Theme::TokyoNight,
            Theme::TokyoNight => Theme::Dracula,
            Theme::Dracula => Theme::Terminal,
        }
    }

    /// The name shown in the legend, which doubles as the value `VIRGA_THEME`
    /// accepts. Lower case because everything else on that bar is.
    pub fn name(self) -> &'static str {
        match self {
            Theme::Terminal => "terminal",
            Theme::CatppuccinMocha => "catppuccin mocha",
            Theme::GruvboxDark => "gruvbox dark",
            Theme::Nord => "nord",
            Theme::TokyoNight => "tokyo night",
            Theme::Dracula => "dracula",
        }
    }

    /// Parse a name from the environment, tolerating the separators people
    /// actually type: `tokyo-night`, `tokyo_night` and `Tokyo Night` are one
    /// theme. Unknown names are rejected rather than guessed at — a typo
    /// silently landing on the wrong palette is worse than being told.
    pub fn from_name(name: &str) -> Option<Self> {
        let wanted = normalise(name);
        Theme::ALL
            .into_iter()
            .find(|theme| normalise(theme.name()) == wanted)
    }

    /// The colours this theme gives the roles. The only place in the app where
    /// a colour literal appears.
    pub fn palette(self) -> Palette {
        match self {
            // Deliberately the sixteen ANSI colours and not their RGB values:
            // the point of this palette is to be whatever the terminal's own
            // scheme says those colours are.
            Theme::Terminal => Palette {
                background: Color::Reset,
                accent: Color::Blue,
                text: Color::White,
                muted: Color::DarkGray,
                selection: Color::Yellow,
                now: Color::LightBlue,
                series: Color::Blue,
                error: Color::Red,
                border: Color::Reset,
            },
            Theme::CatppuccinMocha => Palette {
                background: Color::Rgb(30, 30, 46),   // base
                accent: Color::Rgb(137, 180, 250),    // blue
                text: Color::Rgb(205, 214, 244),      // text
                muted: Color::Rgb(108, 112, 134),     // overlay0
                selection: Color::Rgb(249, 226, 175), // yellow
                now: Color::Rgb(148, 226, 213),       // teal
                series: Color::Rgb(116, 199, 236),    // sapphire
                error: Color::Rgb(243, 139, 168),     // red
                border: Color::Rgb(69, 71, 90),       // surface1
            },
            Theme::GruvboxDark => Palette {
                background: Color::Rgb(40, 40, 40),  // bg0
                accent: Color::Rgb(131, 165, 152),   // aqua
                text: Color::Rgb(235, 219, 178),     // fg1
                muted: Color::Rgb(146, 131, 116),    // gray
                selection: Color::Rgb(250, 189, 47), // bright yellow
                now: Color::Rgb(184, 187, 38),       // bright green
                series: Color::Rgb(131, 165, 152),   // aqua
                error: Color::Rgb(251, 73, 52),      // bright red
                border: Color::Rgb(80, 73, 69),      // bg2
            },
            Theme::Nord => Palette {
                background: Color::Rgb(46, 52, 64), // nord0
                accent: Color::Rgb(136, 192, 208),  // nord8
                text: Color::Rgb(236, 239, 244),    // nord6
                // nord-vim's brightened nord3 rather than nord3 itself, which
                // the spec reserves for invisibles: against nord0 it lands at
                // 1.7:1, and `muted` here carries labels, not indent guides.
                muted: Color::Rgb(97, 110, 136),
                selection: Color::Rgb(235, 203, 139), // nord13
                now: Color::Rgb(163, 190, 140),       // nord14
                series: Color::Rgb(129, 161, 193),    // nord9
                error: Color::Rgb(191, 97, 106),      // nord11
                border: Color::Rgb(67, 76, 94),       // nord2
            },
            Theme::TokyoNight => Palette {
                background: Color::Rgb(26, 27, 38),   // bg
                accent: Color::Rgb(122, 162, 247),    // blue
                text: Color::Rgb(192, 202, 245),      // fg
                muted: Color::Rgb(86, 95, 137),       // comment
                selection: Color::Rgb(224, 175, 104), // yellow
                now: Color::Rgb(158, 206, 106),       // green
                series: Color::Rgb(125, 207, 255),    // cyan
                error: Color::Rgb(247, 118, 142),     // red
                border: Color::Rgb(59, 66, 97),       // bg_highlight
            },
            Theme::Dracula => Palette {
                background: Color::Rgb(40, 42, 54),   // background
                accent: Color::Rgb(189, 147, 249),    // purple
                text: Color::Rgb(248, 248, 242),      // foreground
                muted: Color::Rgb(98, 114, 164),      // comment
                selection: Color::Rgb(241, 250, 140), // yellow
                now: Color::Rgb(80, 250, 123),        // green
                series: Color::Rgb(139, 233, 253),    // cyan
                error: Color::Rgb(255, 85, 85),       // red
                border: Color::Rgb(68, 71, 90),       // current line
            },
        }
    }
}

/// Fold case and treat every separator people might type as a space, so the
/// environment variable is forgiving about which one they picked.
fn normalise(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c == '-' || c == '_' { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// `next` is the cycle and `ALL` is the list of what is in it. If they
    /// disagree, a theme is either unreachable by pressing `t` or missing from
    /// the environment variable, and this is what says so.
    #[test]
    fn cycling_visits_every_theme_and_comes_back() {
        let mut theme = Theme::default();
        let mut seen = Vec::new();

        for _ in 0..Theme::ALL.len() {
            seen.push(theme);
            theme = theme.next();
        }

        assert_eq!(theme, Theme::default(), "the cycle did not come back round");
        assert_eq!(
            seen.iter().copied().collect::<HashSet<_>>(),
            Theme::ALL.into_iter().collect::<HashSet<_>>(),
            "the cycle and ALL disagree about which themes exist"
        );
    }

    /// The default has to be the terminal palette: it is the only one that is
    /// correct without 24-bit colour, so nobody is downgraded by upgrading.
    #[test]
    fn the_default_is_the_terminals_own_colours() {
        assert_eq!(Theme::default(), Theme::Terminal);
    }

    /// The regression guard for the look the app already had. Every one of
    /// these was a literal at a call site before there were themes.
    #[test]
    fn the_terminal_theme_keeps_todays_colours() {
        let p = Theme::Terminal.palette();

        assert_eq!(
            p.background,
            Color::Reset,
            "the terminal palette must not impose a ground of its own"
        );
        assert_eq!(p.accent, Color::Blue);
        assert_eq!(p.text, Color::White);
        assert_eq!(p.muted, Color::DarkGray);
        assert_eq!(p.selection, Color::Yellow);
        assert_eq!(p.now, Color::LightBlue);
        assert_eq!(p.series, Color::Blue);
        assert_eq!(p.error, Color::Red);
        assert_eq!(p.border, Color::Reset, "the border was never painted");
    }

    /// Two themes sharing a name would make the legend ambiguous and the
    /// environment variable arbitrary.
    #[test]
    fn every_theme_has_its_own_name() {
        let names: HashSet<&str> = Theme::ALL.into_iter().map(Theme::name).collect();
        assert_eq!(names.len(), Theme::ALL.len());
    }

    #[test]
    fn every_name_parses_back_to_its_theme() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_name(theme.name()), Some(theme));
        }
    }

    /// The name is rendered into the legend, where a long one costs bindings
    /// their place on the bar. Nothing enforces brevity but this.
    #[test]
    fn names_stay_short_enough_for_the_legend() {
        for theme in Theme::ALL {
            let name = theme.name();
            assert!(name.chars().count() <= 16, "{name:?} is too long a name");
            assert_eq!(name, name.to_lowercase(), "{name:?} is not lower case");
        }
    }

    /// `VIRGA_THEME` is typed by hand into a shell profile, so it accepts the
    /// separators and the casing people actually use.
    #[test]
    fn parsing_a_name_forgives_case_and_separators() {
        for spelling in [
            "tokyo night",
            "Tokyo Night",
            "TOKYO-NIGHT",
            "tokyo_night",
            "  tokyo night  ",
        ] {
            assert_eq!(
                Theme::from_name(spelling),
                Some(Theme::TokyoNight),
                "{spelling:?} was not understood"
            );
        }
    }

    /// A typo must not land silently on some other palette.
    #[test]
    fn an_unknown_name_is_rejected_rather_than_guessed_at() {
        for name in ["", "catppuccin", "tokyo", "solarized", "nordic"] {
            assert_eq!(Theme::from_name(name), None, "{name:?} was accepted");
        }
    }

    /// A role left the same colour as another is a role the reader cannot
    /// distinguish. The three that mark bars — selection, now and ordinary —
    /// sit side by side in both charts, so they are the ones that must differ.
    #[test]
    fn the_three_bar_states_are_distinguishable_in_every_theme() {
        for theme in Theme::ALL {
            let p = theme.palette();
            let states = [p.selection, p.now, p.series];
            let distinct: HashSet<_> = states.iter().collect();

            assert_eq!(
                distinct.len(),
                states.len(),
                "{}: selection, now and series are not three colours",
                theme.name()
            );
        }
    }

    /// Muted is for labels sitting next to readings. If a theme makes them the
    /// same colour, the distinction the layout relies on is gone.
    #[test]
    fn labels_never_match_the_readings_beside_them() {
        for theme in Theme::ALL {
            let p = theme.palette();
            assert_ne!(p.muted, p.text, "{}", theme.name());
            assert_ne!(p.muted, p.accent, "{}", theme.name());
        }
    }

    /// The named palettes bring their own ground, so a light terminal is no
    /// longer allowed to decide whether they are legible — and once the ground
    /// is ours, so is the contrast, which is the thing worth asserting.
    ///
    /// `Terminal` is excluded on purpose: its colours are whatever the user
    /// configured them to be, and there is nothing here to measure.
    #[test]
    fn every_role_clears_its_ground_in_every_named_theme() {
        for theme in Theme::ALL.into_iter().filter(|t| *t != Theme::Terminal) {
            let p = theme.palette();

            // Three floors, because the roles are not doing the same job.
            // Readings have to be read; labels are meant to recede but still be
            // legible beside them; a border is a rule, and being nearly
            // invisible is the point of it.
            let roles = [
                ("accent", p.accent, 3.0),
                ("text", p.text, 3.0),
                ("selection", p.selection, 3.0),
                ("now", p.now, 3.0),
                ("series", p.series, 3.0),
                ("error", p.error, 3.0),
                ("muted", p.muted, 2.4),
                ("border", p.border, 1.4),
            ];

            for (role, colour, floor) in roles {
                let ratio = contrast(colour, p.background);
                assert!(
                    ratio >= floor,
                    "{}: {role} is {ratio:.2}:1 against its own background, \
                     under the {floor:.1}:1 this role has to clear",
                    theme.name()
                );
            }
        }
    }

    /// A background no darker than the foregrounds drawn on it would mean the
    /// palette had been half-inverted, which the ratios above cannot catch:
    /// contrast is symmetric and would be just as happy either way round.
    #[test]
    fn named_grounds_are_the_darkest_thing_in_their_palette() {
        for theme in Theme::ALL.into_iter().filter(|t| *t != Theme::Terminal) {
            let p = theme.palette();
            let ground = luminance(p.background);

            for (role, colour) in [
                ("accent", p.accent),
                ("text", p.text),
                ("muted", p.muted),
                ("selection", p.selection),
                ("now", p.now),
                ("series", p.series),
                ("error", p.error),
                ("border", p.border),
            ] {
                assert!(
                    luminance(colour) > ground,
                    "{}: {role} is darker than the ground it sits on",
                    theme.name()
                );
            }
        }
    }

    /// WCAG relative luminance. Only defined for the 24-bit palettes — an ANSI
    /// colour has no fixed value to measure, which is why `Terminal` is left
    /// out of the tests above.
    fn luminance(colour: Color) -> f64 {
        let Color::Rgb(r, g, b) = colour else {
            panic!("{colour:?} has no measurable luminance");
        };
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    /// WCAG contrast ratio, 1.0 for two identical colours and 21.0 for black
    /// against white.
    fn contrast(a: Color, b: Color) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}
