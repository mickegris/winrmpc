#!/usr/bin/env python3
"""Rebuild `assets/fonts/winrmpc-icons.ttf` from upstream Material Symbols.

The app bundles its own icon font because iced only ever bundles `Iced-Icons.ttf`
on native targets and otherwise resolves families through *system* fonts — so
naming any system font (the old `Segoe UI Symbol`) silently renders tofu boxes
wherever that font isn't installed. See docs/plans/row-action-affordance.md.

This is a *developer* tool, not part of `cargo build`: the resulting .ttf is
committed, and `ui::widgets::icon`'s tests assert the font still carries every
codepoint the UI references. Run it only when the icon set changes.

    python3 -m venv .venv && .venv/bin/pip install fonttools brotli
    .venv/bin/python3 packaging/fonts/build-icon-font.py

Upstream is the Material Symbols Outlined *variable* font (Apache-2.0). Two
things are done to it, both load-bearing:

  * **Axes are pinned to a static instance.** iced never sets variation
    coordinates, so shipping a variable font would leave the rendered weight up
    to whatever default the rasteriser picks.
  * **FILL=0 for every glyph except the status dot.** The outlined style reads
    better at the 15-16px the row buttons use, but `fiber_manual_record` at
    FILL=0 is a hollow ring, and that glyph's whole job is to be a solid dot.
    So it is instanced separately at FILL=1 and merged in.

The family is renamed to `winrmpc Icons` so a system-installed copy of Material
Symbols can never win the family lookup instead of the bundled subset.
"""

import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "assets" / "fonts" / "winrmpc-icons.ttf"

BASE = (
    "https://github.com/google/material-design-icons/raw/master/variablefont/"
    "MaterialSymbolsOutlined%5BFILL%2CGRAD%2Copsz%2Cwght%5D.ttf"
)

FAMILY = "winrmpc Icons"

# Keep in sync with src/ui/widgets/icon.rs — the tests there check both
# directions, so a codepoint added here without a constant (or vice versa)
# fails `cargo test`.
OUTLINE = {
    "play_arrow": 0xE037,
    "add": 0xE145,
    "queue_play_next": 0xE066,
    "playlist_add": 0xE03B,
    "arrow_upward": 0xE5D8,
    "arrow_downward": 0xE5DB,
    "close": 0xE5CD,
    "grid_view": 0xE9B0,
    "list": 0xE896,
    "warning": 0xF083,
    "music_note": 0xE405,
    "queue_music": 0xE03D,
    "remove": 0xE15B,
    "history": 0xE8B3,
    "check": 0xE668,
    "arrow_forward": 0xE5C8,
    # Transport + playback modes (0.4.3): the player bar's controls were text
    # labels reading Prev/Play/Stop/Next.
    "skip_previous": 0xE045,
    "pause": 0xE034,
    "stop": 0xE047,
    "skip_next": 0xE044,
    "arrow_back": 0xE5C4,
    "volume_up": 0xE050,
    "repeat": 0xE040,
    "repeat_one": 0xE041,
    "shuffle": 0xE043,
}
FILLED = {"fiber_manual_record": 0xE061}


def run(*args):
    subprocess.run([sys.executable, "-m", "fontTools", *args], check=True)


def main():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        src = tmp / "upstream.ttf"
        print(f"downloading {BASE}")
        urllib.request.urlretrieve(BASE, src)

        parts = []
        for name, fill, codepoints in (
            ("outline", "0", OUTLINE),
            ("dot", "1", FILLED),
        ):
            static = tmp / f"{name}-static.ttf"
            part = tmp / f"{name}.ttf"
            run(
                "varLib.instancer", str(src),
                f"FILL={fill}", "GRAD=0", "opsz=24", "wght=400",
                "-o", str(static),
            )
            run(
                "subset", str(static),
                "--unicodes=" + ",".join(f"{c:04x}" for c in codepoints.values()),
                f"--output-file={part}",
                "--layout-features=", "--no-hinting", "--name-IDs=*",
                "--recalc-bounds",
            )
            parts.append(str(part))

        merged = tmp / "merged.ttf"
        run("merge", *parts, f"--output-file={merged}")

        from fontTools.ttLib import TTFont

        font = TTFont(merged)
        for rec in font["name"].names:
            if rec.nameID == 1:
                rec.string = FAMILY
            elif rec.nameID == 2:
                rec.string = "Regular"
            elif rec.nameID == 4:
                rec.string = f"{FAMILY} Regular"
            elif rec.nameID == 6:
                rec.string = FAMILY.replace(" ", "") + "-Regular"

        OUT.parent.mkdir(parents=True, exist_ok=True)
        font.save(OUT)

        check = TTFont(OUT)
        cmap = check.getBestCmap()
        want = {**OUTLINE, **FILLED}
        missing = {n: c for n, c in want.items() if c not in cmap}
        if missing:
            raise SystemExit(f"glyphs missing from the subset: {missing}")
        print(
            f"wrote {OUT.relative_to(REPO)} "
            f"({OUT.stat().st_size} bytes, {len(cmap)} glyphs, "
            f"upem={check['head'].unitsPerEm})"
        )


if __name__ == "__main__":
    main()
