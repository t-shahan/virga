pub(super) fn symbol(code: Option<u8>) -> &'static str {
    match code {
        None => " ",
        Some(0) => "○",
        // The dotted circle, not the half-filled ◐: several terminal fonts
        // lack the U+25D0 half circles, and the fallback glyph they substitute
        // advances wider than one cell, shearing the whole row off its grid.
        // The math-operator circle is carried natively by the common coding
        // fonts, and ○ ⊙ ● still reads as a fill ramp from clear to overcast.
        Some(1 | 2) => "⊙",
        Some(3) => "●",
        Some(45 | 48) => "≡",
        Some(51 | 53 | 55 | 56 | 57) => "┆",
        Some(61 | 63 | 65 | 66 | 67 | 80 | 81 | 82) => "│",
        Some(71 | 73 | 75 | 77 | 85 | 86) => "*",
        Some(95 | 96 | 99) => "ϟ",
        Some(_) => "?",
    }
}

/// The full weathergram's sky emoji, two cells wide by ratatui's accounting.
///
/// Most entries carry U+FE0F, the emoji variation selector: their base
/// codepoints measure one cell as text but draw two once a terminal gives
/// them emoji presentation, and the selector is what makes the width crate
/// and the terminal agree on two. The groups mirror [`symbol`] exactly, and
/// a test holds the two tables to the same partition.
pub(super) fn emoji(code: Option<u8>) -> &'static str {
    match code {
        None => " ",
        Some(0) => "☀\u{fe0f}",
        Some(1 | 2) => "⛅",
        Some(3) => "☁\u{fe0f}",
        Some(45 | 48) => "🌫\u{fe0f}",
        Some(51 | 53 | 55 | 56 | 57) => "🌦\u{fe0f}",
        Some(61 | 63 | 65 | 66 | 67 | 80 | 81 | 82) => "🌧\u{fe0f}",
        Some(71 | 73 | 75 | 77 | 85 | 86) => "❄\u{fe0f}",
        Some(95 | 96 | 99) => "⛈\u{fe0f}",
        Some(_) => "❓",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Line;

    #[test]
    fn groups_documented_wmo_codes_into_one_cell_conditions() {
        for (codes, expected) in [
            (&[0][..], "○"),
            (&[1, 2], "⊙"),
            (&[3], "●"),
            (&[45, 48], "≡"),
            (&[51, 53, 55, 56, 57], "┆"),
            (&[61, 63, 65, 66, 67, 80, 81, 82], "│"),
            (&[71, 73, 75, 77, 85, 86], "*"),
            (&[95, 96, 99], "ϟ"),
        ] {
            for code in codes {
                assert_eq!(symbol(Some(*code)), expected, "code {code}");
            }
        }
    }

    #[test]
    fn absence_is_blank_and_an_unknown_reported_code_is_a_question() {
        assert_eq!(symbol(None), " ");
        assert_eq!(symbol(Some(200)), "?");
    }

    #[test]
    fn every_drawn_symbol_occupies_one_terminal_cell() {
        for code in [0, 1, 3, 45, 51, 61, 71, 95, 200] {
            assert_eq!(Line::from(symbol(Some(code))).width(), 1, "code {code}");
        }
    }

    #[test]
    fn every_drawn_emoji_occupies_two_terminal_cells() {
        for code in [0, 1, 3, 45, 51, 61, 71, 95, 200] {
            assert_eq!(Line::from(emoji(Some(code))).width(), 2, "code {code}");
        }
        assert_eq!(emoji(None), " ");
    }

    /// The emoji table must partition the codes exactly as the text table
    /// does, or the two layouts would disagree about what counts as a change
    /// of condition.
    #[test]
    fn emoji_groups_mirror_the_text_groups() {
        for a in 0..=110u8 {
            for b in 0..=110u8 {
                assert_eq!(
                    symbol(Some(a)) == symbol(Some(b)),
                    emoji(Some(a)) == emoji(Some(b)),
                    "codes {a} and {b} grouped differently"
                );
            }
        }
    }
}
