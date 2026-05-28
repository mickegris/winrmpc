//! Application icon: circular dark-navy background with three cyan
//! equalizer bars (#4fc3f7).  The same design is baked into the exe as a
//! Windows resource (build.rs) and set as the runtime window icon here.

/// Generate 32×32 RGBA pixel data (R, G, B, A byte order).
pub fn rgba_pixels() -> Vec<u8> {
    make_rgba(32)
}

fn make_rgba(size: u32) -> Vec<u8> {
    let mut px = vec![0u8; (size * size * 4) as usize];

    let cx = (size as f32 - 1.0) / 2.0;
    let cy = cx;
    let r = size as f32 / 2.0 - 0.5;

    // Background circle: #1a1a2e
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if (dx * dx + dy * dy).sqrt() <= r {
                let i = ((y * size + x) * 4) as usize;
                px[i] = 0x1a; px[i+1] = 0x1a; px[i+2] = 0x2e; px[i+3] = 0xff;
            }
        }
    }

    // Three equalizer bars: #4fc3f7
    // Positions are expressed as fractions of 32 and scaled.
    let s = size as f32 / 32.0;
    let bars: [(f32, f32, f32); 3] = [
        (8.0, 11.0, 10.0),   // (x, y_start, height)
        (14.0, 7.0, 18.0),
        (20.0, 10.0, 12.0),
    ];
    let bar_w = (4.0 * s).max(1.0) as u32;

    for (bx, by, bh) in bars {
        let bx = (bx * s) as u32;
        let by = (by * s) as u32;
        let bh = (bh * s).max(1.0) as u32;

        for y in by..by + bh {
            for x in bx..bx + bar_w {
                if x < size && y < size {
                    let i = ((y * size + x) * 4) as usize;
                    px[i] = 0x4f; px[i+1] = 0xc3; px[i+2] = 0xf7; px[i+3] = 0xff;
                }
            }
        }
    }

    px
}

/// Build the iced window icon from the generated pixel data.
pub fn make_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_rgba(rgba_pixels(), 32, 32).ok()
}
