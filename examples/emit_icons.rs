//! Regenerate the installable icon assets under `packaging/linux/icons/`.
//!
//! ```sh
//! cargo run --example emit_icons          # writes packaging/linux/icons/
//! cargo run --example emit_icons -- /tmp  # writes /tmp/hicolor/...
//! ```
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
