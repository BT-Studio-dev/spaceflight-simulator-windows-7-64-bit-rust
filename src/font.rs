//! A 5x7 bitmap font, hand-drawn so the simulator needs no font files, no
//! GDI calls and no external assets. Glyphs are authored as readable bit
//! strings and compiled into a lookup table once at start-up.

use std::sync::OnceLock;

/// Width of one glyph in pixels (before scaling).
pub const GLYPH_W: usize = 5;
/// Height of one glyph in pixels (before scaling).
pub const GLYPH_H: usize = 7;
/// Horizontal advance including the 1px gap.
pub const ADVANCE: usize = 6;

/// Glyph bit rows: 7 rows of 5 characters, `#` = lit.
const RAW: &[(char, [&str; GLYPH_H])] = &[
    (' ', ["     "; 7]),
    ('A', [" ### ", "#   #", "#   #", "#####", "#   #", "#   #", "#   #"]),
    ('B', ["#### ", "#   #", "#   #", "#### ", "#   #", "#   #", "#### "]),
    ('C', [" ### ", "#   #", "#    ", "#    ", "#    ", "#   #", " ### "]),
    ('D', ["#### ", "#   #", "#   #", "#   #", "#   #", "#   #", "#### "]),
    ('E', ["#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####"]),
    ('F', ["#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#    "]),
    ('G', [" ### ", "#   #", "#    ", "#  ##", "#   #", "#   #", " ####"]),
    ('H', ["#   #", "#   #", "#   #", "#####", "#   #", "#   #", "#   #"]),
    ('I', [" ### ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### "]),
    ('J', ["  ###", "   # ", "   # ", "   # ", "   # ", "#  # ", " ##  "]),
    ('K', ["#   #", "#  # ", "# #  ", "##   ", "# #  ", "#  # ", "#   #"]),
    ('L', ["#    ", "#    ", "#    ", "#    ", "#    ", "#    ", "#####"]),
    ('M', ["#   #", "## ##", "# # #", "# # #", "#   #", "#   #", "#   #"]),
    ('N', ["#   #", "##  #", "# # #", "#  ##", "#   #", "#   #", "#   #"]),
    ('O', [" ### ", "#   #", "#   #", "#   #", "#   #", "#   #", " ### "]),
    ('P', ["#### ", "#   #", "#   #", "#### ", "#    ", "#    ", "#    "]),
    ('Q', [" ### ", "#   #", "#   #", "#   #", "# # #", "#  # ", " ## #"]),
    ('R', ["#### ", "#   #", "#   #", "#### ", "# #  ", "#  # ", "#   #"]),
    ('S', [" ####", "#    ", "#    ", " ### ", "    #", "    #", "#### "]),
    ('T', ["#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  "]),
    ('U', ["#   #", "#   #", "#   #", "#   #", "#   #", "#   #", " ### "]),
    ('V', ["#   #", "#   #", "#   #", "#   #", "#   #", " # # ", "  #  "]),
    ('W', ["#   #", "#   #", "#   #", "# # #", "# # #", "## ##", "#   #"]),
    ('X', ["#   #", "#   #", " # # ", "  #  ", " # # ", "#   #", "#   #"]),
    ('Y', ["#   #", "#   #", " # # ", "  #  ", "  #  ", "  #  ", "  #  "]),
    ('Z', ["#####", "    #", "   # ", "  #  ", " #   ", "#    ", "#####"]),
    ('0', [" ### ", "#   #", "#  ##", "# # #", "##  #", "#   #", " ### "]),
    ('1', ["  #  ", " ##  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### "]),
    ('2', [" ### ", "#   #", "    #", "   # ", "  #  ", " #   ", "#####"]),
    ('3', ["#####", "   # ", "  #  ", "   # ", "    #", "#   #", " ### "]),
    ('4', ["   # ", "  ## ", " # # ", "#  # ", "#####", "   # ", "   # "]),
    ('5', ["#####", "#    ", "#### ", "    #", "    #", "#   #", " ### "]),
    ('6', ["  ## ", " #   ", "#    ", "#### ", "#   #", "#   #", " ### "]),
    ('7', ["#####", "    #", "   # ", "  #  ", " #   ", " #   ", " #   "]),
    ('8', [" ### ", "#   #", "#   #", " ### ", "#   #", "#   #", " ### "]),
    ('9', [" ### ", "#   #", "#   #", " ####", "    #", "   # ", " ##  "]),
    ('.', ["     ", "     ", "     ", "     ", "     ", " ### ", " ### "]),
    (',', ["     ", "     ", "     ", "     ", " ### ", "  #  ", " #   "]),
    (':', ["     ", " ### ", " ### ", "     ", " ### ", " ### ", "     "]),
    (';', ["     ", " ### ", " ### ", "     ", " ### ", "  #  ", " #   "]),
    ('!', ["  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "     ", "  #  "]),
    ('?', [" ### ", "#   #", "    #", "   # ", "  #  ", "     ", "  #  "]),
    ('-', ["     ", "     ", "     ", "#####", "     ", "     ", "     "]),
    ('+', ["     ", "  #  ", "  #  ", "#####", "  #  ", "  #  ", "     "]),
    ('=', ["     ", "     ", "#####", "     ", "#####", "     ", "     "]),
    ('/', ["    #", "    #", "   # ", "  #  ", " #   ", "#    ", "#    "]),
    ('\\', ["#    ", "#    ", " #   ", "  #  ", "   # ", "    #", "    #"]),
    ('_', ["     ", "     ", "     ", "     ", "     ", "     ", "#####"]),
    ('(', ["   # ", "  #  ", " #   ", " #   ", " #   ", "  #  ", "   # "]),
    (')', [" #   ", "  #  ", "   # ", "   # ", "   # ", "  #  ", " #   "]),
    ('[', [" ### ", " #   ", " #   ", " #   ", " #   ", " #   ", " ### "]),
    (']', [" ### ", "   # ", "   # ", "   # ", "   # ", "   # ", " ### "]),
    ('<', ["   # ", "  #  ", " #   ", "#    ", " #   ", "  #  ", "   # "]),
    ('>', [" #   ", "  #  ", "   # ", "    #", "   # ", "  #  ", " #   "]),
    ('%', ["##  #", "##  #", "   # ", "  #  ", " #   ", "#  ##", "#  ##"]),
    ('*', ["     ", "# # #", " ### ", "#####", " ### ", "# # #", "     "]),
    ('#', [" # # ", " # # ", "#####", " # # ", "#####", " # # ", " # # "]),
    ('&', [" ##  ", "#  # ", "# #  ", " #   ", "# # #", "#  # ", " ## #"]),
    ('@', [" ### ", "#   #", "# ###", "# # #", "# ###", "#    ", " ### "]),
    ('^', ["  #  ", " # # ", "#   #", "     ", "     ", "     ", "     "]),
    ('~', ["     ", "     ", " #   ", "# # #", "   # ", "     ", "     "]),
    ('|', ["  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  "]),
    ('$', ["  #  ", " ####", "# #  ", " ### ", "  # #", "#### ", "  #  "]),
    ('\'', ["  #  ", "  #  ", "     ", "     ", "     ", "     ", "     "]),
    ('"', [" # # ", " # # ", "     ", "     ", "     ", "     ", "     "]),
    ('°', [" ##  ", "#  # ", " ##  ", "     ", "     ", "     ", "     "]),
    ('·', ["     ", "     ", "  #  ", "     ", "     ", "     ", "     "]),
    ('←', ["     ", "  #  ", " #   ", "#####", " #   ", "  #  ", "     "]),
    ('→', ["     ", "  #  ", "   # ", "#####", "   # ", "  #  ", "     "]),
    ('↑', ["  #  ", " ### ", "# # #", "  #  ", "  #  ", "  #  ", "  #  "]),
    ('↓', ["  #  ", "  #  ", "  #  ", "  #  ", "# # #", " ### ", "  #  "]),
    ('`', ["  #  ", "   # ", "     ", "     ", "     ", "     ", "     "]),
    ('{', ["   ##", "  #  ", "  #  ", " #   ", "  #  ", "  #  ", "   ##"]),
    ('}', ["##   ", "  #  ", "  #  ", "   # ", "  #  ", "  #  ", "##   "]),
    ('■', ["     ", " ### ", " ### ", " ### ", " ### ", " ### ", "     "]),
    ('●', ["     ", " ### ", "#####", "#####", "#####", " ### ", "     "]),
];

fn table() -> &'static [(char, [u8; GLYPH_H])] {
    static TABLE: OnceLock<Vec<(char, [u8; GLYPH_H])>> = OnceLock::new();
    TABLE.get_or_init(|| {
        RAW.iter()
            .map(|(ch, rows)| {
                let mut bits = [0u8; GLYPH_H];
                for (i, row) in rows.iter().enumerate() {
                    let mut v = 0u8;
                    for (j, c) in row.chars().enumerate() {
                        if j < GLYPH_W && c != ' ' {
                            v |= 1 << (GLYPH_W - 1 - j);
                        }
                    }
                    bits[i] = v;
                }
                (*ch, bits)
            })
            .collect()
    })
}

fn glyph(ch: char) -> Option<&'static [u8; GLYPH_H]> {
    let ch = if ch.is_ascii_lowercase() {
        ch.to_ascii_uppercase()
    } else if ch == '\u{a0}' {
        ' '
    } else {
        ch
    };
    table()
        .iter()
        .find(|(c, _)| *c == ch)
        .map(|(_, g)| g)
}

/// Width in pixels of `text` at the given scale (no trailing gap).
pub fn text_width(text: &str, scale: i32) -> i32 {
    let mut chars = 0;
    let mut longest = 0;
    for ch in text.chars() {
        if ch == '\n' {
            longest = longest.max(chars);
            chars = 0;
        } else {
            chars += 1;
        }
    }
    longest = longest.max(chars);
    if longest == 0 {
        0
    } else {
        ((longest * ADVANCE) as i32 - 1) * scale
    }
}

pub fn text_height(lines: usize, scale: i32) -> i32 {
    if lines == 0 {
        0
    } else {
        (lines as i32 * (GLYPH_H as i32 + 2) - 2) * scale
    }
}

pub fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

/// Returns, for each glyph pixel, whether it is lit. Used by the renderer; it
/// walks rows directly so no allocation is required.
#[inline]
pub fn glyph_pixel(ch: char, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x >= GLYPH_W as i32 || y >= GLYPH_H as i32 {
        return false;
    }
    match glyph(ch) {
        Some(g) => g[y as usize] & (1 << (GLYPH_W as i32 - 1 - x)) != 0,
        None => false,
    }
}

/// Iterate over the lit pixels of `text`, calling `f(x, y, char)` with pixel
/// coordinates relative to the top-left of the text block (pre-scale).
pub fn for_each_pixel(text: &str, mut f: impl FnMut(i32, i32)) {
    for (li, line) in text.split('\n').enumerate() {
        let oy = li as i32 * (GLYPH_H as i32 + 2);
        for (ci, ch) in line.chars().enumerate() {
            let ox = ci as i32 * ADVANCE as i32;
            for y in 0..GLYPH_H as i32 {
                for x in 0..GLYPH_W as i32 {
                    if glyph_pixel(ch, x, y) {
                        f(ox + x, oy + y);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_well_formed() {
        for (ch, rows) in RAW {
            assert_eq!(rows.len(), GLYPH_H, "glyph {ch} row count");
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    GLYPH_W,
                    "glyph {ch} row {i} must be {GLYPH_W} wide, got {row:?}"
                );
                assert!(
                    row.chars().all(|c| c == ' ' || c == '#'),
                    "glyph {ch} row {i} has invalid chars"
                );
            }
            if *ch != ' ' {
                assert!(
                    rows.iter().any(|r| r.contains('#')),
                    "glyph {ch} is blank"
                );
            }
        }
    }

    #[test]
    fn every_printable_ascii_is_covered() {
        for c in 32u8..127 {
            let ch = c as char;
            assert!(
                glyph(ch).is_some(),
                "missing glyph for printable ascii {ch:?} ({c})"
            );
        }
    }

    #[test]
    fn lowercase_maps_to_uppercase() {
        assert_eq!(glyph('a'), glyph('A'));
        assert_eq!(glyph('z'), glyph('Z'));
    }

    #[test]
    fn widths() {
        assert_eq!(text_width("A", 1), 5);
        assert_eq!(text_width("AB", 1), 11);
        assert_eq!(text_width("AB", 2), 22);
        assert_eq!(text_width("", 1), 0);
        assert_eq!(text_width("AB\nABCD", 1), 23);
        assert_eq!(line_count("A\nB"), 2);
        assert_eq!(text_height(2, 1), 16);
    }
}
