# Row-action button affordance ("+", "⏭", "☰")

Part of [cross-platform-and-ui-0.4.2](cross-platform-and-ui-0.4.2.md).

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

## How to confirm on the real OS

The whole point of A is that this cannot be verified by reading code:

- **Before, on macOS and Linux**: screenshot an album track list. Expect tofu
  boxes (□) where `▶ + ⏭ ☰` should be. Confirm which of the *other* glyphs in
  the Cause 2 inventory also fail — that determines how wide the audit is.
- **After A**: all icons render on Windows, macOS and Linux, and the app no
  longer names any system font.
- **After B**: hovering each row action shows its tooltip; check the tooltip
  doesn't clip at the window edge for the right-most buttons.
