# Row-action button affordance ("+", "⏭", "☰")

Part of [cross-platform-and-ui-0.4.2](cross-platform-and-ui-0.4.2.md).

> **Status (2026-08-14): implemented — A, B, C and D. E was considered and
> declined.** See "What was actually built" at the bottom. The one thing still
> outstanding is the acceptance criterion itself: **the icons have not been
> seen rendered by iced on any platform**, only rasterised directly from the
> font file. That check needs the running app.

## The report

> Buttons for adding to playlist and add next look weird, hard to understand
> what they actually do before you click.

There are two separate causes, and the second one is worse than the first.

## Cause 1 — the glyphs are ambiguous

Every song row carries up to four bare glyph buttons with no label and no
tooltip (`album.rs:179-182`, `search.rs:88-91`,
`playlist_detail.rs:95-115`, `queue.rs:129-139`):

| Glyph | Message | What a user reasonably reads it as |
|---|---|---|
| `▶` | `PlaySong` | Play — fine |
| `+` | `QueueAddOnly` | Add — to *what*? Ambiguous with `☰` |
| `⏭` | `QueueAddNext` | **Skip to next track.** This is the universal transport glyph for "next", and it's the wrong metaphor for "insert after the current track" |
| `☰` | `OpenAddToPlaylist` | A hamburger menu. Nothing about it suggests playlists |

`⏭` is the most misleading: the player bar's own Next control sits a few
inches away doing the *actual* skip (it's a text button, `player_bar.rs:53`,
so there's no visual collision — but the mental model collision is real).

`☰` is also doing double duty — `album_grid.rs:117` uses the same glyph for
the **List** layout toggle. One glyph, two unrelated meanings, same screen.

The one place that gets it right is Now Playing, which uses a **text** link:
`link("+ Add to Playlist", …)` (`now_playing.rs:102`). Nobody misreads that.

## Cause 2 — on macOS and Linux the glyphs don't render at all

`src/ui/widgets/link.rs:33-41`:

```rust
/// Font used for the play/add glyphs. The bundled iced default font lacks
/// `▶`/`＋`, rendering them as tofu boxes; Segoe UI Symbol (always present on
/// Windows) has them.
const ICON_FONT: iced::Font = iced::Font::with_name("Segoe UI Symbol");
```

The comment states the problem and then names a **Windows-only** font as the
solution. On macOS and Linux `Segoe UI Symbol` does not exist, so the lookup
falls back to the bundled default — which is precisely the font the comment
says renders tofu. So on the two platforms in question these buttons are not
merely unclear, they are **empty boxes**.

Worth noting the inconsistency this creates: `icon_btn` is the *only* thing
that uses `ICON_FONT`. Glyphs elsewhere — `▦ Grid`/`☰ List`
(`album_grid.rs:117`), `−` (`player_bar.rs:110`), `♪ Instrumental` and
`▤ Playing from` (`now_playing.rs:382`, `:93`), `●` (`snapcast.rs:176`,
`app.rs:3382`), `⚠` (`log.rs:80`), `↑`/`↓`/`✕`/`▲`/`▼` in the queue and
playlist views — go through the default font with no icon font at all. Some of
those are in more widely supported Unicode blocks and may render; the arrows
and `✕` generally do, `▦` and `▤` are much less certain. **This needs an
actual look on macOS and Linux** — the font fallback chain is
platform-dependent and can't be settled by reading source.

## Plan

### A. Bundle an icon font (fixes cause 2 properly)

Stop depending on any system font. `iced::application(...)` exposes
`.font(impl Into<Cow<'static, [u8]>>)` (`iced-0.13.1/src/application.rs:208`),
so a font can be embedded with `include_bytes!` and registered at startup:

```rust
iced::application("winrmpc", App::update, App::view)
    .font(include_bytes!("../assets/fonts/icons.ttf").as_slice())
    …
```

Then `ICON_FONT` becomes `Font::with_name("<bundled family name>")` and
renders identically on all three platforms.

Two options for the font itself:

1. **A subsetted open-licence icon font** (Material Symbols, Lucide, Phosphor —
   all OFL/MIT). Subset to the ~12 glyphs actually used; that's a few KB.
   Gives purpose-built icons rather than repurposed Unicode.
2. **A general font with good symbol coverage** (DejaVu Sans, ~700 KB
   unsubsetted) keeping the current Unicode code points.

Recommend **1**. It solves cause 1 and cause 2 together — a dedicated
"playlist-add" icon exists in every icon set and communicates far better than
`☰` ever will. It also removes the "one glyph, two meanings" collision with the
layout toggle.

Whichever is chosen, **audit every non-ASCII literal in `src/ui/`** (the list
in Cause 2 above is the full inventory) and decide per glyph: route it through
the icon font, or replace it with text. Leaving them on the default font is
what created this bug.

### B. Tooltips on every row action (fixes cause 1 directly)

`iced::widget::tooltip` exists in 0.13 (`iced_widget-0.13.4/src/lib.rs:35`).
This is the most direct answer to "hard to understand before you click" —
hover reveals the meaning without spending row width.

Extend `link.rs` with a tooltip-carrying variant and make it the default:

```rust
pub fn icon_btn_tip<'a>(
    label: &'static str,
    tip: &'static str,
    on_press: Message,
) -> Element<'a, Message>
```

Keep `icon_btn` as a thin wrapper for the handful of cases where a tooltip is
noise (`▲`/`▼` reorder arrows, arguably).

Proposed tooltip text — the wording is the deliverable here, so it should be
consistent everywhere the action appears:

| Action | Tooltip |
|---|---|
| `PlaySong` | **Play now** |
| `QueueAddOnly` | **Add to end of queue** |
| `QueueAddNext` | **Play next** (insert after the current track) |
| `OpenAddToPlaylist` | **Add to playlist…** |
| `QueueRemove` | **Remove from queue** |
| `QueueMoveUp`/`Down` | **Move up** / **Move down** |
| `PlaylistMoveSongUp`/`Down` | **Move up in playlist** / **Move down in playlist** |

The ellipsis on "Add to playlist…" is doing real work — it's the only one of
these that opens a picker instead of acting immediately.

### C. Re-pick the two bad glyphs

Even with tooltips, the resting-state glyph should not actively mislead:

- **`⏭` → a queue-insert icon.** Whatever the chosen icon set calls
  "playlist-add"/"queue-next" — a list with an arrow entering it. Anything but
  the transport skip glyph.
- **`☰` → a playlist icon.** A list with a `+`. Frees `☰` to mean "list
  layout" unambiguously in `album_grid.rs`.

### D. Consistency sweep

Two things are inconsistent between views and should be settled while here:

- **`playlist_detail.rs:115-125` hand-rolls its own button** for
  `OpenAddToPlaylist` instead of calling `icon_btn`, duplicating the style
  block. Replace with the shared helper.
- **Row actions differ per view**: album and search rows offer
  play/add/add-next/playlist; playlist_detail offers play/add/add-next plus
  move/playlist; queue offers move/playlist/remove. Some of that is inherent
  (you can't reorder an album), but `QueueAddOnly` missing from the queue view
  and `OpenAddToPlaylist` present in all four should be a deliberate table, not
  an accident. Write the table down in CLAUDE.md once decided.

### E. Consider labels over icons in low-density rows

Now Playing's `+ Add to Playlist` text link is the clearest control of the set.
Album and playlist rows have width to spare at the 1000px minimum window size.
Not proposing it as the default — four text buttons per row would be heavy —
but worth a look during implementation for the one or two rows where it fits.

## What was actually built

**A — bundled icon font.** Option 1 from the plan: a subsetted open-licence
icon set, not a general font. `assets/fonts/winrmpc-icons.ttf` is a 17-glyph,
~2.6 KB subset of Material Symbols Outlined (Apache-2.0), registered once via
`iced::application().font(icon::FONT_BYTES)`. `src/ui/widgets/icon.rs` owns it
and exposes one `pub const` per glyph.

Three build details that were not obvious from the plan:

- **The upstream font is variable, and had to be instanced.** iced never sets
  variation coordinates, so shipping a variable font leaves the rendered
  weight to whatever default the rasteriser picks. Axes are pinned
  (`FILL`/`GRAD`/`opsz`/`wght`) before subsetting.
- **`FILL=0` for everything except the status dot.** Outlined reads better at
  15-16px, but `fiber_manual_record` at `FILL=0` is a hollow ring, and that
  glyph's whole job is to be a solid dot. It is instanced separately at
  `FILL=1` and merged in.
- **The family is renamed to `winrmpc Icons`.** Left as "Material Symbols
  Outlined", a system-installed copy of the *full* family could win the family
  lookup and supply glyphs this subset doesn't have — a subtler version of the
  same class of bug the plan is about.

`packaging/fonts/build-icon-font.py` does all of it and is committed;
`fonttools` is a dev-only dependency, not part of `cargo build`.

**The audit was completed, and it was wider than the four row actions.** Every
non-ASCII literal in `src/ui/` was reviewed and sorted into three piles:

- *Routed through the icon font* — the four row actions, plus `↑↓▲▼✕×` (queue,
  playlist, server and playlist-delete rows), `▦`/`☰` (layout toggle), `●`
  (Snapcast and server status), `⚠` (Log), `♪`/`▤` (Now Playing), `−` (player
  bar crossfade), `🕐` (History link) and `✓` (Log filter).
- *Left as text* — `…`, `–`, `—`, `·`. General Punctuation, covered
  essentially everywhere; the ellipsis in "Add to playlist…" is deliberate.
- *Re-picked* — `→` was **not** left as text. It is in the Arrows block, whose
  coverage in system sans fonts is much less certain than punctuation, so the
  Outputs view's "move to partition" buttons now use `ARROW_FORWARD`.

Mixed icon+text strings had to become two widgets each (`link_icon`,
`layout_toggle`, `centered_icon_note`, and the Log view's slow-command
marker): one `text` carries one font, so `"🕐 History"` could only ever render
one half correctly. The Log marker also needed a fixed-width cell so the
monospace lines still start at the same x whether marked or not.

**B — tooltips.** `icon_btn_tip(glyph, tip, msg)` is now the default for row
actions, with `icon_btn_danger` for destructive ones (it keeps the `ERROR`
colour the hand-rolled delete buttons had). Tooltips are placed `Top`, not to
the side, because the right-most actions sit near the window edge. `icon_btn`
survives as the untooltipped primitive.

**C — the two bad glyphs.** `⏭` → `PLAY_NEXT` (`queue_play_next`), `☰` →
`ADD_PLAYLIST` (`playlist_add`), which frees `LIST` to mean list-layout
unambiguously.

**D — consistency sweep.** `playlist_detail.rs`'s two hand-rolled buttons now
go through the shared helpers. The per-view action set is written down as a
table in CLAUDE.md, with the two gaps stated as decisions rather than left as
accidents: the queue has no "add to end of queue" (those rows *are* the
queue), and only reorderable lists get move arrows. Browser file rows gained
the full four actions.

**E — declined.** Text labels in low-density rows were considered and not
done: with tooltips carrying the meaning, four text buttons per row costs
width in every view for a benefit only the widest windows would see. Now
Playing keeps its `+ Add to Playlist` text link.

Test count went 185 → 189. The four new ones parse the committed TTF's `cmap`
directly and assert every constant resolves to a glyph, that the font carries
no glyph without a constant, that no two constants collide, and that the
family name is the bundled one — the same "committed binary asset can't
drift" shape as `packaged_png_assets_match_the_generator`.

## How to confirm on the real OS

The whole point of A is that this cannot be verified by reading code. **This
has not been done yet.** The glyphs were rasterised straight from the built
font file to confirm each codepoint draws the intended icon, but that proves
the font is correct, *not* that iced resolves the bundled family at runtime —
which is the actual claim:

- **Before, on macOS and Linux**: screenshot an album track list. Expect tofu
  boxes (□) where `▶ + ⏭ ☰` should be. Confirm which of the *other* glyphs in
  the Cause 2 inventory also fail — that determines how wide the audit is.
- **After A**: all icons render on Windows, macOS and Linux, and the app no
  longer names any system font.
- **After B**: hovering each row action shows its tooltip; check the tooltip
  doesn't clip at the window edge for the right-most buttons.
