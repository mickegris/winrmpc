// The application icon's geometry, in one place.
//
// A dark-navy circle (`#1a1a2e`) with three cyan equalizer bars (`#4fc3f7`),
// drawn procedurally so every size is free and no binary asset has to be kept
// in step with the code.
//
// **This file is `include!`d by `build.rs` and by `examples/emit_icons.rs`**,
// which constrains it in two ways:
//
//   1. No dependency on the crate or on external crates — plain `std` only,
//      no `use crate::…`, no `iced`, no `image`.
//   2. No inner (`//!`) doc comments, here or anywhere below. `include!`
//      splices this text into the middle of another file, and inner docs are
//      only legal at the top of one. That is why this header is `//`.
//
// Three consumers share it:
//
//   1. `src/icon.rs` → the runtime window icon (iced/winit).
//   2. `build.rs` → the BMP-in-ICO Windows resource embedded by `winres`.
//   3. `examples/emit_icons.rs` → the PNG/SVG assets under `packaging/linux/`.
//
// Before this existed, (1) and (2) were copy-pasted siblings drawing the same
// design twice — consistent by luck rather than construction. Adding a third
// copy for the Linux assets is what made sharing worth the `include!` trick.

/// Reverse-DNS application id.
///
/// It lives here rather than in `main.rs` because this is the one file the
/// app, the build script and the asset emitter all see, and the id has to be
/// **byte-identical in three places** or the Linux icon silently doesn't
/// resolve:
///
/// - the Wayland `app_id` / X11 `WM_CLASS` set in `main.rs`,
/// - the basename of the installed `.desktop` file,
/// - the basename of the installed icon files.
///
/// A compositor finds a window's icon by matching `app_id` to a `.desktop`
/// filename, then reads that file's `Icon=` key. Any mismatch in the chain and
/// the window falls back to a generic placeholder with no error anywhere.
pub const APP_ID: &str = "io.github.mickegris.winrmpc";

/// Background circle colour, `#1a1a2e`, as RGB.
pub const BG_RGB: [u8; 3] = [0x1a, 0x1a, 0x2e];

/// Equalizer bar colour, `#4fc3f7`, as RGB.
pub const BAR_RGB: [u8; 3] = [0x4f, 0xc3, 0xf7];

/// The design is authored on a 32×32 grid and scaled to whatever size is
/// asked for; every measurement below is in grid units.
pub const GRID: f32 = 32.0;

/// Bar width, in grid units.
pub const BAR_W: f32 = 4.0;

/// The three bars as `(x, y_start, height)` in grid units.
pub const BARS: [(f32, f32, f32); 3] = [
    (8.0, 11.0, 10.0),
    (14.0, 7.0, 18.0),
    (20.0, 10.0, 12.0),
];

/// Sizes emitted into the freedesktop `hicolor` icon theme.
///
/// 16–48 are the sizes panels and window lists actually pick; 256 and 512 are
/// what GNOME Software and file managers use for large previews. Each is drawn
/// at its native size rather than downscaled, so the hard-edged circle stays
/// crisp instead of picking up resampling fringes.
pub const HICOLOR_SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

/// Generate `size`×`size` **top-down RGBA** pixel data (R, G, B, A byte order).
///
/// Top-down is the orientation iced and PNG both want. `build.rs` flips rows
/// and swizzles to BGRA itself, because that is what the BMP inside an ICO
/// requires — those two transforms are the *only* format-specific code, and
/// keeping them at the consumer keeps this function single-purpose.
pub fn rgba_pixels(size: u32) -> Vec<u8> {
    let mut px = vec![0u8; (size * size * 4) as usize];

    let cx = (size as f32 - 1.0) / 2.0;
    let cy = cx;
    let r = size as f32 / 2.0 - 0.5;

    // Background circle.
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if (dx * dx + dy * dy).sqrt() <= r {
                let i = ((y * size + x) * 4) as usize;
                px[i] = BG_RGB[0];
                px[i + 1] = BG_RGB[1];
                px[i + 2] = BG_RGB[2];
                px[i + 3] = 0xff;
            }
        }
    }

    // Equalizer bars, drawn over the circle.
    let s = size as f32 / GRID;
    let bar_w = (BAR_W * s).max(1.0) as u32;

    for (bx, by, bh) in BARS {
        let bx = (bx * s) as u32;
        let by = (by * s) as u32;
        let bh = (bh * s).max(1.0) as u32;

        for y in by..by + bh {
            for x in bx..bx + bar_w {
                if x < size && y < size {
                    let i = ((y * size + x) * 4) as usize;
                    px[i] = BAR_RGB[0];
                    px[i + 1] = BAR_RGB[1];
                    px[i + 2] = BAR_RGB[2];
                    px[i + 3] = 0xff;
                }
            }
        }
    }

    px
}

/// The same design as a scalable SVG, for `hicolor/scalable/apps/`.
///
/// Built from the constants above rather than hand-written, so it cannot drift
/// from the raster output. The bars all sit comfortably inside the circle at
/// these coordinates, so no clip path is needed — the raster version relies on
/// exactly the same fact (it clips only at the image bounds).
pub fn svg() -> String {
    let g = GRID;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{g}\" height=\"{g}\" \
         viewBox=\"0 0 {g} {g}\">\n",
    );
    s.push_str(&format!(
        "  <circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>\n",
        g / 2.0,
        g / 2.0,
        g / 2.0,
        hex(BG_RGB),
    ));
    for (bx, by, bh) in BARS {
        s.push_str(&format!(
            "  <rect x=\"{bx}\" y=\"{by}\" width=\"{BAR_W}\" height=\"{bh}\" fill=\"{}\"/>\n",
            hex(BAR_RGB),
        ));
    }
    s.push_str("</svg>\n");
    s
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}
