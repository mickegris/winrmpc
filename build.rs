// The icon geometry, shared verbatim with the app (`src/icon.rs`) and the
// Linux asset emitter (`examples/emit_icons.rs`). A build script can't `use`
// a crate module, so it is pulled in textually; `src/icon_design.rs` is
// dependency-free precisely so this works.
include!("src/icon_design.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/icon_design.rs");

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

/// Convert the shared top-down RGBA design into the bottom-up BGRA layout a
/// BMP-inside-an-ICO requires.
///
/// These two transforms — the row flip and the channel swizzle — are the only
/// icon code that is genuinely Windows-specific. The drawing itself comes from
/// `rgba_pixels`, so the ICO can no longer drift from the window icon.
fn make_bgra(size: u32) -> Vec<u8> {
    let rgba = rgba_pixels(size);
    let mut px = vec![0u8; rgba.len()];

    for y in 0..size {
        // BMP rows run bottom-up.
        let bmp_y = size - 1 - y;
        for x in 0..size {
            let src = ((y * size + x) * 4) as usize;
            let dst = ((bmp_y * size + x) * 4) as usize;
            px[dst] = rgba[src + 2];     // B
            px[dst + 1] = rgba[src + 1]; // G
            px[dst + 2] = rgba[src];     // R
            px[dst + 3] = rgba[src + 3]; // A
        }
    }

    px
}
