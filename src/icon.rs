//! The runtime window icon.
//!
//! The design itself lives in [`crate::icon_design`], shared with `build.rs`
//! (Windows ICO resource) and `examples/emit_icons.rs` (Linux PNG/SVG assets).
//!
//! Note what this buys you per platform, because it is not uniform:
//!
//! | Platform | Effect |
//! |---|---|
//! | Windows | Works. `build.rs` also embeds an ICO for Explorer/taskbar |
//! | Linux / X11 | Works — winit sets `_NET_WM_ICON` |
//! | Linux / Wayland | **Nothing.** winit's `set_window_icon` is an empty function there; the icon comes from the installed `.desktop` file matched by `app_id` (see `packaging/linux/`) |
//! | macOS | **Nothing.** winit documents that macOS has no window icons; it comes from the `.app` bundle's `.icns` |

use crate::icon_design;

/// The size handed to iced for the runtime window icon.
///
/// Left at the original 32 deliberately: the drawing has hard (un-antialiased)
/// edges, so a larger source downscaled by the window manager would look
/// *different* from the Windows ICO rather than merely sharper, and "same icon
/// everywhere" is the property worth keeping. Raising it is a reasonable
/// follow-up, but it should come with anti-aliasing and a look on real HiDPI.
const WINDOW_ICON_SIZE: u32 = 32;

/// Generate 32×32 RGBA pixel data (R, G, B, A byte order).
pub fn rgba_pixels() -> Vec<u8> {
    icon_design::rgba_pixels(WINDOW_ICON_SIZE)
}

/// Build the iced window icon from the generated pixel data.
pub fn make_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_rgba(rgba_pixels(), WINDOW_ICON_SIZE, WINDOW_ICON_SIZE).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn packaged_icon_path(size: u32) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("packaging/linux/icons/hicolor")
            .join(format!("{size}x{size}"))
            .join("apps")
            .join(format!("{}.png", icon_design::APP_ID))
    }

    #[test]
    fn window_icon_is_generated() {
        assert!(make_icon().is_some());
        assert_eq!(
            rgba_pixels().len(),
            (WINDOW_ICON_SIZE * WINDOW_ICON_SIZE * 4) as usize
        );
    }

    #[test]
    fn centre_is_opaque_and_corners_are_transparent() {
        let size = 32u32;
        let px = icon_design::rgba_pixels(size);
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * size + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2], px[i + 3]]
        };
        // A circle inscribed in the square leaves the corners empty.
        assert_eq!(at(0, 0)[3], 0, "top-left corner should be transparent");
        assert_eq!(at(size - 1, 0)[3], 0, "top-right corner should be transparent");
        // The middle bar covers the centre.
        assert_eq!(at(16, 16), [0x4f, 0xc3, 0xf7, 0xff], "centre should be bar-coloured");
    }

    /// The checked-in PNGs under `packaging/linux/` must match what the
    /// generator produces right now.
    ///
    /// This is the guard that makes checking in generated binaries safe: change
    /// the design and this test fails until the assets are regenerated with
    /// `cargo run --example emit_icons`. It compares *decoded pixels*, not
    /// encoded bytes, so a change in the `image` crate's PNG encoder can't
    /// cause a spurious failure.
    #[test]
    fn packaged_png_assets_match_the_generator() {
        for size in icon_design::HICOLOR_SIZES {
            let path = packaged_icon_path(size);
            let file = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) => panic!(
                    "missing packaged icon {}: {e}\n\
                     regenerate with: cargo run --example emit_icons",
                    path.display()
                ),
            };
            let decoded = image::load_from_memory(&file)
                .unwrap_or_else(|e| panic!("{} is not a readable PNG: {e}", path.display()))
                .to_rgba8();

            assert_eq!(decoded.width(), size, "{} has the wrong width", path.display());
            assert_eq!(decoded.height(), size, "{} has the wrong height", path.display());
            assert_eq!(
                decoded.into_raw(),
                icon_design::rgba_pixels(size),
                "{} is stale — regenerate with: cargo run --example emit_icons",
                path.display()
            );
        }
    }

    #[test]
    fn svg_covers_the_circle_and_every_bar() {
        let svg = icon_design::svg();
        assert!(svg.starts_with("<svg"), "should be an SVG document");
        assert!(svg.contains("viewBox=\"0 0 32 32\""));
        assert_eq!(svg.matches("<circle").count(), 1);
        assert_eq!(svg.matches("<rect").count(), icon_design::BARS.len());
        assert!(svg.contains("#1a1a2e"), "background colour missing");
        assert!(svg.contains("#4fc3f7"), "bar colour missing");
    }
}
