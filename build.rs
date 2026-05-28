fn main() {
    // Only embed a Windows resource on Windows targets.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let ico_path = std::path::Path::new(&out_dir).join("winrmpc.ico");

    let ico = build_ico();
    std::fs::write(&ico_path, ico).expect("failed to write icon");

    let mut res = winres::WindowsResource::new();
    res.set_icon(ico_path.to_str().unwrap());
    if let Err(e) = res.compile() {
        // Non-fatal: icon embedding requires rc.exe / windres to be on PATH.
        println!("cargo:warning=icon embedding skipped: {e}");
    }
}

// ── ICO generation (BMP-in-ICO, no extra crates needed) ──────────────────────

fn build_ico() -> Vec<u8> {
    let bmp16 = make_bmp(16);
    let bmp32 = make_bmp(32);

    // ICONDIR (6 bytes) + 2 × ICONDIRENTRY (16 bytes each) + image data
    let count: u16 = 2;
    let header_size = 6usize + count as usize * 16;
    let offset16 = header_size as u32;
    let offset32 = offset16 + bmp16.len() as u32;

    let mut ico = Vec::with_capacity(header_size + bmp16.len() + bmp32.len());

    // ICONDIR
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    ico.extend_from_slice(&count.to_le_bytes());

    ico.extend_from_slice(&icondirentry(16, &bmp16, offset16));
    ico.extend_from_slice(&icondirentry(32, &bmp32, offset32));

    ico.extend_from_slice(&bmp16);
    ico.extend_from_slice(&bmp32);
    ico
}

fn icondirentry(size: u8, bmp: &[u8], offset: u32) -> [u8; 16] {
    let mut e = [0u8; 16];
    e[0] = size;  // width
    e[1] = size;  // height
    // e[2] color count = 0 (true color)
    // e[3] reserved = 0
    e[4..6].copy_from_slice(&1u16.to_le_bytes());   // planes
    e[6..8].copy_from_slice(&32u16.to_le_bytes());  // bit count
    e[8..12].copy_from_slice(&(bmp.len() as u32).to_le_bytes());
    e[12..16].copy_from_slice(&offset.to_le_bytes());
    e
}

/// Build a 32-bit BGRA BMP (no file header) suitable for embedding in ICO.
/// Includes BITMAPINFOHEADER + pixel data (bottom-up) + AND mask (all 0s).
fn make_bmp(size: u32) -> Vec<u8> {
    let pixel_data = make_bgra(size); // bottom-up BGRA
    let and_mask_row = ((size + 31) / 32) * 4; // rows padded to DWORD boundary
    let and_mask_size = (and_mask_row * size) as usize;

    let mut bmp = Vec::with_capacity(40 + pixel_data.len() + and_mask_size);

    // BITMAPINFOHEADER
    bmp.extend_from_slice(&40u32.to_le_bytes());              // biSize
    bmp.extend_from_slice(&(size as i32).to_le_bytes());      // biWidth
    bmp.extend_from_slice(&(size as i32 * 2).to_le_bytes());  // biHeight (×2 for XOR+AND)
    bmp.extend_from_slice(&1u16.to_le_bytes());               // biPlanes
    bmp.extend_from_slice(&32u16.to_le_bytes());              // biBitCount
    bmp.extend_from_slice(&0u32.to_le_bytes());               // biCompression (BI_RGB)
    bmp.extend_from_slice(&0u32.to_le_bytes());               // biSizeImage
    bmp.extend_from_slice(&0i32.to_le_bytes());               // biXPelsPerMeter
    bmp.extend_from_slice(&0i32.to_le_bytes());               // biYPelsPerMeter
    bmp.extend_from_slice(&0u32.to_le_bytes());               // biClrUsed
    bmp.extend_from_slice(&0u32.to_le_bytes());               // biClrImportant

    bmp.extend_from_slice(&pixel_data);
    bmp.extend(std::iter::repeat(0u8).take(and_mask_size));  // AND mask (all visible)
    bmp
}

/// Generate bottom-up BGRA pixels for the icon at the given size.
fn make_bgra(size: u32) -> Vec<u8> {
    let mut px = vec![0u8; (size * size * 4) as usize];

    let cx = (size as f32 - 1.0) / 2.0;
    let cy = cx;
    let r = size as f32 / 2.0 - 0.5;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            // BMP is bottom-up
            let bmp_y = size - 1 - y;
            let i = ((bmp_y * size + x) * 4) as usize;

            if (dx * dx + dy * dy).sqrt() <= r {
                // Background: #1a1a2e → BGRA = 0x2e, 0x1a, 0x1a, 0xff
                px[i] = 0x2e; px[i+1] = 0x1a; px[i+2] = 0x1a; px[i+3] = 0xff;
            }
        }
    }

    // Three equalizer bars: #4fc3f7 → BGRA = 0xf7, 0xc3, 0x4f, 0xff
    let s = size as f32 / 32.0;
    let bars: [(f32, f32, f32); 3] = [
        (8.0, 11.0, 10.0),
        (14.0, 7.0, 18.0),
        (20.0, 10.0, 12.0),
    ];
    let bar_w = (4.0 * s).max(1.0) as u32;

    for (bx, by, bh) in bars {
        let bx = (bx * s) as u32;
        let by = (by * s) as u32;
        let bh = (bh * s).max(1.0) as u32;

        for y in by..by + bh {
            let bmp_y = size - 1 - y;
            for x in bx..bx + bar_w {
                if x < size && y < size {
                    let i = ((bmp_y * size + x) * 4) as usize;
                    px[i] = 0xf7; px[i+1] = 0xc3; px[i+2] = 0x4f; px[i+3] = 0xff;
                }
            }
        }
    }

    px
}
