//! The app's colour palette, in light and dark.
//!
//! # Why these are functions and not constants
//!
//! They used to be `const Color`s, and most of the ~320 uses are direct
//! `.color(...)` calls in view code — not style closures. iced only hands
//! `&Theme` to a *style closure*, so those call sites have no theme in scope
//! and no way to ask which mode is active. Threading a `&Palette` through
//! every `views::*::view()` signature would touch every view and every call
//! site in `app.rs` for a value that is almost always just forwarded.
//!
//! So the mode lives in one process-global `AtomicU8` and the accessors index
//! into a two-element table. The load is lock-free and costs nothing next to
//! laying out the widget that uses the colour. The cost is a global, which is
//! why [`set_dark_mode`] is called from exactly two places — startup and the
//! Settings toggle — and why the tests set it explicitly rather than assuming
//! a default.

use iced::Color;
use std::sync::atomic::{AtomicBool, Ordering};

/// Every colour the app draws with, in one mode.
///
/// Adding a field here means adding it to **both** [`DARK`] and [`LIGHT`] —
/// the struct is what makes that non-optional.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub bg_hover: Color,

    pub row_even: Color,
    pub row_odd: Color,
    pub row_playing: Color,

    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_disabled: Color,

    pub accent: Color,
    pub accent_dim: Color,

    pub success: Color,
    pub warning: Color,
    pub error: Color,

    pub border: Color,
    pub progress_bg: Color,
    pub progress_fill: Color,
}

/// The original palette — the app shipped dark-only through 0.4.2.
pub const DARK: Palette = Palette {
    bg_primary: Color::from_rgb(0.11, 0.11, 0.14),
    bg_secondary: Color::from_rgb(0.14, 0.14, 0.18),
    bg_tertiary: Color::from_rgb(0.18, 0.18, 0.22),
    bg_hover: Color::from_rgb(0.22, 0.22, 0.28),

    row_even: Color::from_rgb(0.13, 0.13, 0.16),
    row_odd: Color::from_rgb(0.15, 0.15, 0.19),
    row_playing: Color::from_rgb(0.17, 0.19, 0.25),

    text_primary: Color::from_rgb(0.93, 0.93, 0.95),
    text_secondary: Color::from_rgb(0.62, 0.62, 0.68),
    text_muted: Color::from_rgb(0.42, 0.42, 0.48),
    text_disabled: Color::from_rgb(0.24, 0.24, 0.28),

    accent: Color::from_rgb(0.31, 0.76, 0.97),
    accent_dim: Color::from_rgb(0.20, 0.50, 0.65),

    success: Color::from_rgb(0.30, 0.85, 0.50),
    warning: Color::from_rgb(0.95, 0.75, 0.25),
    error: Color::from_rgb(0.90, 0.30, 0.30),

    border: Color::from_rgb(0.25, 0.25, 0.30),
    progress_bg: Color::from_rgb(0.20, 0.20, 0.25),
    progress_fill: Color::from_rgb(0.31, 0.76, 0.97),
};

/// The light palette.
///
/// **Not a mechanical inversion.** Three deliberate departures:
///
/// - **`accent` is much darker than dark mode's `#4fc3f7`.** That cyan was
///   chosen against near-black and is close to illegible on white — and since
///   `accent` is also the *playing track's* text colour, that is a legibility
///   problem, not a taste one.
/// - **`row_playing` is a blue tint, not a grey step.** On a light background
///   the three row colours sit within a few percent of each other, so a
///   neutral highlight would be invisible; the hue is what carries it.
/// - **`warning` and `success` are darkened well past their dark-mode values.**
///   A colour picked to glow on black has almost no contrast on white.
pub const LIGHT: Palette = Palette {
    bg_primary: Color::from_rgb(0.97, 0.97, 0.98),
    bg_secondary: Color::from_rgb(0.94, 0.94, 0.96),
    bg_tertiary: Color::from_rgb(0.90, 0.90, 0.93),
    bg_hover: Color::from_rgb(0.85, 0.86, 0.90),

    row_even: Color::from_rgb(0.99, 0.99, 1.00),
    row_odd: Color::from_rgb(0.955, 0.955, 0.975),
    row_playing: Color::from_rgb(0.84, 0.90, 0.97),

    text_primary: Color::from_rgb(0.10, 0.10, 0.14),
    text_secondary: Color::from_rgb(0.34, 0.34, 0.41),
    text_muted: Color::from_rgb(0.50, 0.50, 0.57),
    text_disabled: Color::from_rgb(0.72, 0.72, 0.76),

    accent: Color::from_rgb(0.05, 0.40, 0.63),
    accent_dim: Color::from_rgb(0.42, 0.60, 0.72),

    success: Color::from_rgb(0.08, 0.50, 0.26),
    warning: Color::from_rgb(0.60, 0.42, 0.02),
    error: Color::from_rgb(0.72, 0.13, 0.13),

    border: Color::from_rgb(0.79, 0.79, 0.84),
    progress_bg: Color::from_rgb(0.84, 0.84, 0.88),
    progress_fill: Color::from_rgb(0.05, 0.40, 0.63),
};

/// The active mode. Dark is the default because that is what every install
/// before 0.4.3 was.
static DARK_MODE: AtomicBool = AtomicBool::new(true);

/// Switch palettes. Called from startup (reading `config.theme.dark_mode`) and
/// from the Settings toggle — nowhere else.
pub fn set_dark_mode(dark: bool) {
    DARK_MODE.store(dark, Ordering::Relaxed);
}

pub fn is_dark_mode() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

/// The palette for the active mode.
pub fn active() -> &'static Palette {
    if is_dark_mode() {
        &DARK
    } else {
        &LIGHT
    }
}

/// Serialises the tests that mutate the global mode against the ones that
/// read it.
///
/// The mode is process-global and Rust runs tests in threads within one
/// process, so without this a test that flips to light for a microsecond can
/// make an unrelated colour assertion elsewhere read the wrong palette. Flaky
/// colour tests would be worse than no colour tests.
#[cfg(test)]
pub static TEST_MODE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub struct AppColors;

/// Each accessor is `active().field`. Kept as associated functions on
/// `AppColors` so the ~320 call sites read the same as before bar the `()`.
macro_rules! color_accessors {
    ($($name:ident),* $(,)?) => {
        impl AppColors {
            $(
                #[allow(non_snake_case)]
                pub fn $name() -> Color { active().$name }
            )*
        }
    };
}

color_accessors!(
    bg_primary,
    bg_secondary,
    bg_tertiary,
    bg_hover,
    row_even,
    row_odd,
    row_playing,
    text_primary,
    text_secondary,
    text_muted,
    text_disabled,
    accent,
    accent_dim,
    success,
    warning,
    error,
    border,
    progress_bg,
    progress_fill,
);

#[cfg(test)]
mod tests {
    use super::*;

    /// Restores the mode afterwards, because it is process-global and the
    /// tests run in threads within one process.
    fn with_mode<T>(dark: bool, f: impl FnOnce() -> T) -> T {
        let _guard = TEST_MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = is_dark_mode();
        set_dark_mode(dark);
        let out = f();
        set_dark_mode(prev);
        out
    }

    #[test]
    fn the_accessors_follow_the_mode() {
        with_mode(true, || assert_eq!(AppColors::bg_primary(), DARK.bg_primary));
        with_mode(false, || {
            assert_eq!(AppColors::bg_primary(), LIGHT.bg_primary)
        });
    }

    /// The relationship that matters most, asserted for **both** palettes.
    ///
    /// A playing-row colour equal to either zebra stripe is invisible on half
    /// the rows, and one equal to the hover colour makes hovering do nothing
    /// visible. Light mode is where this is easy to get wrong, because on a
    /// light background all three sit within a few percent of each other.
    #[test]
    fn the_playing_row_is_distinguishable_in_both_palettes() {
        for (name, p) in [("dark", DARK), ("light", LIGHT)] {
            assert_ne!(p.row_playing, p.row_even, "{name}: playing == even stripe");
            assert_ne!(p.row_playing, p.row_odd, "{name}: playing == odd stripe");
            assert_ne!(p.row_playing, p.bg_hover, "{name}: playing == hover");
            assert_ne!(p.row_even, p.row_odd, "{name}: stripes are identical");
        }
    }

    /// Text has to be readable against the background it is drawn on. This is
    /// a coarse luminance check, not a WCAG contrast ratio — enough to catch
    /// the real mistake, which is pasting a dark-mode colour into the light
    /// palette and shipping white-on-white.
    #[test]
    fn foreground_and_background_are_on_opposite_sides_in_both_palettes() {
        fn luma(c: Color) -> f32 {
            0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
        }
        for (name, p) in [("dark", DARK), ("light", LIGHT)] {
            let bg = luma(p.bg_primary);
            for (label, fg) in [
                ("text_primary", p.text_primary),
                ("text_secondary", p.text_secondary),
                ("accent", p.accent),
                ("error", p.error),
                ("success", p.success),
                ("warning", p.warning),
            ] {
                assert!(
                    (luma(fg) - bg).abs() > 0.25,
                    "{name}: {label} has too little contrast against bg_primary \
                     ({:.2} vs {:.2})",
                    luma(fg),
                    bg
                );
            }
        }
    }

    /// `accent` is the playing track's text colour, so it is held to the
    /// stricter of the two roles it plays.
    #[test]
    fn the_light_accent_is_dark_enough_to_read_on_white() {
        fn luma(c: Color) -> f32 {
            0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
        }
        assert!(
            luma(LIGHT.accent) < 0.45,
            "light accent is too bright to read as text on a light row"
        );
        assert!(
            luma(DARK.accent) > 0.55,
            "dark accent is too dim to read as text on a dark row"
        );
    }
}
