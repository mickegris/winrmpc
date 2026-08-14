//! The bundled icon font and the glyphs the UI draws from it.
//!
//! **Every non-decorative glyph in the UI must come from here.** The app used to
//! name `Segoe UI Symbol` for its row-action glyphs, which exists only on
//! Windows; iced bundles just `Iced-Icons.ttf` on native targets and resolves
//! every other family through *system* fonts, so on macOS and Linux that lookup
//! fell back to a font the code's own comment said renders tofu boxes. The
//! buttons were not merely unclear there — they were empty rectangles.
//!
//! Bundling removes the system-font dependency outright: `FONT_BYTES` is
//! registered at startup (`main.rs`) and the glyphs render identically on all
//! three platforms. Regenerate with `packaging/fonts/build-icon-font.py`.

use iced::widget::{text, Text};
use iced::Font;

/// The bundled subset, registered once at startup via `iced::application().font()`.
pub const FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/winrmpc-icons.ttf");

/// Family name recorded inside [`FONT_BYTES`]. Deliberately *not* the upstream
/// "Material Symbols Outlined": a system-installed copy of the full family
/// could otherwise win the lookup and supply glyphs this subset doesn't have.
pub const FAMILY: &str = "winrmpc Icons";

/// The font handle to hand to `text(...).font(...)`.
pub const FONT: Font = Font::with_name(FAMILY);

/// Default size for a row-action glyph. A touch larger than the 15px body text
/// the buttons used to use — these are drawn for 24px and turn muddy below ~16.
pub const SIZE: u16 = 16;

// Glyphs are named for what they *do* here, not for the upstream icon name
// (given in the comment), so a call site reads as intent.

/// `play_arrow` — play this track now.
pub const PLAY: &str = "\u{e037}";
/// `add` — append to the end of the queue.
pub const ADD_QUEUE: &str = "\u{e145}";
/// `queue_play_next` — insert directly after the current track.
///
/// Replaces `⏭`, which is the universal transport glyph for *skip to next* and
/// sat a few inches from the player bar's actual Next control.
pub const PLAY_NEXT: &str = "\u{e066}";
/// `playlist_add` — open the "add to stored playlist" picker.
///
/// Replaces `☰`, which read as a hamburger menu and collided with the layout
/// toggle's own `☰`.
pub const ADD_PLAYLIST: &str = "\u{e03b}";
/// `arrow_upward` — move this row up.
pub const MOVE_UP: &str = "\u{e5d8}";
/// `arrow_downward` — move this row down.
pub const MOVE_DOWN: &str = "\u{e5db}";
/// `close` — remove/delete this row.
pub const REMOVE: &str = "\u{e5cd}";
/// `grid_view` — the cover-grid layout toggle.
pub const GRID: &str = "\u{e9b0}";
/// `list` — the compact-list layout toggle.
pub const LIST: &str = "\u{e896}";
/// `fiber_manual_record` — a filled status dot. The only glyph instanced at
/// `FILL=1`; outlined it is a hollow ring, which is not what a status dot is.
pub const DOT: &str = "\u{e061}";
/// `warning` — the slow-command marker in the Log view.
pub const WARNING: &str = "\u{f083}";
/// `music_note` — the "instrumental, no lyrics" marker.
pub const MUSIC_NOTE: &str = "\u{e405}";
/// `queue_music` — the "playing from <playlist>" marker.
pub const QUEUE_MUSIC: &str = "\u{e03d}";
/// `remove` — the volume-down step in the player bar.
pub const MINUS: &str = "\u{e15b}";
/// `history` — the Recently Played link.
pub const HISTORY: &str = "\u{e8b3}";
/// `check` — an affirmative marker (the Log view's "MPD only" filter).
pub const CHECK: &str = "\u{e668}";
/// `arrow_forward` — "move this to <target>" (the Outputs view's partition
/// buttons). The only glyph here that replaced a *punctuation* character
/// (`→`, U+2192) rather than a symbol: plain typography like `…`, `–`, `—` and
/// `·` stays as text, but an arrow lives in a Unicode block whose coverage in
/// system sans fonts is far less certain.
pub const ARROW_FORWARD: &str = "\u{e5c8}";

/// Every glyph above, for the tests that keep the committed font honest.
pub const ALL: &[(&str, &str)] = &[
    ("PLAY", PLAY),
    ("ADD_QUEUE", ADD_QUEUE),
    ("PLAY_NEXT", PLAY_NEXT),
    ("ADD_PLAYLIST", ADD_PLAYLIST),
    ("MOVE_UP", MOVE_UP),
    ("MOVE_DOWN", MOVE_DOWN),
    ("REMOVE", REMOVE),
    ("GRID", GRID),
    ("LIST", LIST),
    ("DOT", DOT),
    ("WARNING", WARNING),
    ("MUSIC_NOTE", MUSIC_NOTE),
    ("QUEUE_MUSIC", QUEUE_MUSIC),
    ("MINUS", MINUS),
    ("HISTORY", HISTORY),
    ("CHECK", CHECK),
    ("ARROW_FORWARD", ARROW_FORWARD),
];

/// An icon glyph as a `Text` widget at the default row-action size.
pub fn icon<'a>(glyph: &'static str) -> Text<'a> {
    icon_sized(glyph, SIZE)
}

/// An icon glyph at an explicit size.
pub fn icon_sized<'a>(glyph: &'static str, size: u16) -> Text<'a> {
    text(glyph).size(size).font(FONT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal `cmap` format-4 reader — enough to answer "does the committed
    /// font actually contain this codepoint". The font is built with a single
    /// format-4 subtable (platform 3, encoding 1); anything else means the
    /// generator changed and this test should be revisited rather than relaxed.
    fn glyph_ids(font: &[u8]) -> std::collections::HashMap<u32, u16> {
        let be16 = |o: usize| u16::from_be_bytes([font[o], font[o + 1]]);
        let be32 = |o: usize| {
            u32::from_be_bytes([font[o], font[o + 1], font[o + 2], font[o + 3]])
        };

        // sfnt table directory → locate `cmap`.
        let num_tables = be16(4) as usize;
        let cmap = (0..num_tables)
            .map(|i| 12 + i * 16)
            .find(|&rec| &font[rec..rec + 4] == b"cmap")
            .map(|rec| be32(rec + 8) as usize)
            .expect("font has no cmap table");

        // Encoding records → the Windows/BMP subtable.
        let n_enc = be16(cmap + 2) as usize;
        let sub = (0..n_enc)
            .map(|i| cmap + 4 + i * 8)
            .find(|&rec| be16(rec) == 3 && be16(rec + 2) == 1)
            .map(|rec| cmap + be32(rec + 4) as usize)
            .expect("font has no (3, 1) cmap subtable");

        assert_eq!(be16(sub), 4, "expected a format-4 cmap subtable");

        let seg_count = be16(sub + 6) as usize / 2;
        let end_codes = sub + 14;
        let start_codes = end_codes + seg_count * 2 + 2;
        let id_deltas = start_codes + seg_count * 2;
        let id_range_offsets = id_deltas + seg_count * 2;

        let mut out = std::collections::HashMap::new();
        for seg in 0..seg_count {
            let end = be16(end_codes + seg * 2);
            let start = be16(start_codes + seg * 2);
            if start == 0xFFFF {
                continue;
            }
            let delta = be16(id_deltas + seg * 2);
            let range_off_at = id_range_offsets + seg * 2;
            let range_off = be16(range_off_at);

            for c in start..=end {
                let gid = if range_off == 0 {
                    c.wrapping_add(delta)
                } else {
                    // Offsets are measured from the idRangeOffset slot itself.
                    let at = range_off_at
                        + range_off as usize
                        + (c - start) as usize * 2;
                    match be16(at) {
                        0 => continue,
                        g => g.wrapping_add(delta),
                    }
                };
                if gid != 0 {
                    out.insert(c as u32, gid);
                }
            }
        }
        out
    }

    #[test]
    fn every_glyph_constant_exists_in_the_bundled_font() {
        let cmap = glyph_ids(FONT_BYTES);
        for (name, glyph) in ALL {
            let mut chars = glyph.chars();
            let c = chars.next().expect("glyph constant is empty");
            assert!(
                chars.next().is_none(),
                "{name} is more than one char; the icon font maps single codepoints"
            );
            assert!(
                cmap.contains_key(&(c as u32)),
                "{name} (U+{:04X}) is not in assets/fonts/winrmpc-icons.ttf — \
                 rerun packaging/fonts/build-icon-font.py",
                c as u32
            );
        }
    }

    /// The reverse direction: a codepoint in the font with no constant is dead
    /// weight, and usually means someone added a glyph to the build script and
    /// forgot the Rust side.
    #[test]
    fn the_bundled_font_carries_no_glyph_the_ui_cannot_name() {
        let cmap = glyph_ids(FONT_BYTES);
        let known: std::collections::HashSet<u32> = ALL
            .iter()
            .map(|(_, g)| g.chars().next().unwrap() as u32)
            .collect();
        let orphans: Vec<String> = cmap
            .keys()
            .filter(|c| !known.contains(c))
            .map(|c| format!("U+{c:04X}"))
            .collect();
        assert!(
            orphans.is_empty(),
            "font contains glyphs with no constant in ui::widgets::icon: {orphans:?}"
        );
    }

    #[test]
    fn glyph_constants_are_unique() {
        let mut seen = std::collections::HashMap::new();
        for (name, glyph) in ALL {
            if let Some(prev) = seen.insert(*glyph, *name) {
                panic!("{name} and {prev} are the same glyph {glyph:?}");
            }
        }
    }

    /// Guards the reason this module exists at all: naming a system font is the
    /// bug. If this ever fails, the tofu regression is back.
    #[test]
    fn the_icon_font_is_bundled_not_a_system_family() {
        assert!(!FONT_BYTES.is_empty());
        // Platform-3 name records are UTF-16BE, so search for the encoded form
        // rather than treating the font as text.
        let encoded: Vec<u8> = FAMILY
            .encode_utf16()
            .flat_map(|u| u.to_be_bytes())
            .collect();
        assert!(
            FONT_BYTES
                .windows(encoded.len())
                .any(|w| w == encoded.as_slice()),
            "the bundled font's name table should carry {FAMILY}; \
             if the family was renamed, update FAMILY to match"
        );
    }
}
