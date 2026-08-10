//! A block-character font for the hero temperature. Digits and a minus sign
//! are the whole alphabet a thermometer needs.

pub(super) const DIGIT_ROWS: usize = 5;
/// Columns one character occupies once rendered: seven of glyph plus the
/// separator `big_digits` appends. Callers size their columns from this rather
/// than restating it.
pub(super) const CELL_WIDTH: u16 = 8;

/// One 7x5 block glyph. At three cells wide the digits rendered far taller
/// than they were broad, since terminal cells are roughly twice as tall as
/// they are wide. Only digits and a minus sign are needed for temperatures.
fn glyph(c: char) -> [&'static str; DIGIT_ROWS] {
    match c {
        '0' => ["███████", "██   ██", "██   ██", "██   ██", "███████"],
        '1' => ["   ██  ", "   ██  ", "   ██  ", "   ██  ", "   ██  "],
        '2' => ["███████", "     ██", "███████", "██     ", "███████"],
        '3' => ["███████", "     ██", "███████", "     ██", "███████"],
        '4' => ["██   ██", "██   ██", "███████", "     ██", "     ██"],
        '5' => ["███████", "██     ", "███████", "     ██", "███████"],
        '6' => ["███████", "██     ", "███████", "██   ██", "███████"],
        '7' => ["███████", "     ██", "     ██", "     ██", "     ██"],
        '8' => ["███████", "██   ██", "███████", "██   ██", "███████"],
        '9' => ["███████", "██   ██", "███████", "     ██", "███████"],
        '-' => ["       ", "       ", "███████", "       ", "       "],
        _ => ["       "; DIGIT_ROWS],
    }
}

/// Renders `text` as block digits, one `String` per output row.
pub(super) fn big_digits(text: &str) -> [String; DIGIT_ROWS] {
    let mut rows: [String; DIGIT_ROWS] = Default::default();

    for c in text.chars() {
        for (row, part) in rows.iter_mut().zip(glyph(c)) {
            row.push_str(part);
            row.push(' ');
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row must be the same width or Alignment::Center shears the digits,
    /// which is exactly how the hero temperature broke once before.
    #[test]
    fn rows_are_all_the_same_width() {
        for text in ["7", "91", "107", "-12", ""] {
            let rows = big_digits(text);
            let width = rows[0].chars().count();
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row.chars().count(), width, "{text:?} row {i} differs");
            }
            assert_eq!(
                width,
                text.chars().count() * CELL_WIDTH as usize,
                "{text:?} does not match CELL_WIDTH"
            );
        }
    }

    #[test]
    fn unknown_characters_render_as_blanks_rather_than_panicking() {
        let rows = big_digits("?");
        assert!(rows.iter().all(|r| r.trim().is_empty()));
    }

    /// The glyph table must agree with DIGIT_ROWS, or a row would be dropped.
    #[test]
    fn every_glyph_has_the_declared_number_of_rows() {
        for c in "0123456789-?".chars() {
            assert_eq!(glyph(c).len(), DIGIT_ROWS, "glyph {c:?}");
        }
    }
}
