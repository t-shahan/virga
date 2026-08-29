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
}
