//! Regenerate the installable icon assets under `packaging/linux/icons/`.
//!
//! ```sh
//! cargo run --example emit_icons                     # packaging/linux/icons/
//! cargo run --example emit_icons -- /tmp             # /tmp/hicolor/...
//! cargo run --example emit_icons -- --iconset /tmp/winrmpc.iconset
//! ```
//!
//! `--iconset` writes the macOS layout instead — `icon_NxN.png` and
//! `icon_NxN@2x.png` — ready for `iconutil -c icns`. It is a mode of *this*
//! generator rather than a separate script so all three platforms' icons keep
//! coming from one source; that is the property `packaged_png_assets_match_the
//! _generator` exists to protect.
//!
//! The output is checked into the repository so packagers and `install.sh`
//! don't need a Rust toolchain to get at it — and `icon::tests::
//! packaged_png_assets_match_the_generator` fails if the checked-in files ever
//! fall behind this generator, which is what makes committing generated
//! binaries safe here.
//!
//! This is an `example` rather than a flag on the app because winrmpc is a GUI
//! binary with no argument handling, and a one-shot developer tool shouldn't
//! be the reason to add some.

// A binary crate's examples can't `use` the binary's modules, so the shared
// design is pulled in textually — the same trick `build.rs` uses, and the
// reason `src/icon_design.rs` depends on nothing but `std`.
include!("../src/icon_design.rs");

use std::path::{Path, PathBuf};

/// The sizes an `.icns` wants, as (points, scale). 1024px comes from
/// 512@2x — above `HICOLOR_SIZES`' top end, but the design is procedural on a
/// 32-unit grid so it scales without a source image.
const ICONSET: [(u32, u32); 5] = [(16, 1), (32, 1), (128, 1), (256, 1), (512, 1)];

fn emit_iconset(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    for (points, _) in ICONSET {
        for scale in [1u32, 2] {
            let px_size = points * scale;
            let name = if scale == 1 {
                format!("icon_{points}x{points}.png")
            } else {
                format!("icon_{points}x{points}@{scale}x.png")
            };
            let path = dir.join(name);
            let px = rgba_pixels(px_size);
            let img = image::RgbaImage::from_raw(px_size, px_size, px)
                .ok_or("pixel buffer did not match the requested dimensions")?;
            img.save(&path)?;
            report(&path);
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--iconset") {
        let dir = args
            .get(1)
            .map(PathBuf::from)
            .ok_or("--iconset needs an output directory")?;
        return emit_iconset(&dir);
    }

    let root: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packaging/linux/icons")
        });

    for size in HICOLOR_SIZES {
        let dir = root.join("hicolor").join(format!("{size}x{size}")).join("apps");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{APP_ID}.png"));

        let px = rgba_pixels(size);
        let img = image::RgbaImage::from_raw(size, size, px)
            .ok_or("pixel buffer did not match the requested dimensions")?;
        img.save(&path)?;
        report(&path);
    }

    let scalable = root.join("hicolor/scalable/apps");
    std::fs::create_dir_all(&scalable)?;
    let svg_path = scalable.join(format!("{APP_ID}.svg"));
    std::fs::write(&svg_path, svg())?;
    report(&svg_path);

    Ok(())
}

/// Print the path relative to the repo root when it is under it, so the output
/// is readable regardless of where the command was run from.
fn report(path: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let shown = path.strip_prefix(root).unwrap_or(path);
    println!("wrote {}", shown.display());
}
