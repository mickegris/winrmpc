# UI polish — 0.4.3

Umbrella for the 0.4.3 planning round, on `improve/ui-0.4.3`. **Plans only —
nothing implemented yet.**

Where 0.4.2 was about correctness that happened to be invisible (glyphs that
didn't render, storage that silently didn't save, TLS that pulled in OpenSSL),
0.4.3 is about the interface itself: controls that don't look like what they
do, names that aren't clickable when they obviously should be, and a music
player that can't be paused from the keyboard.

| # | Plan | Origin |
|---|---|---|
| 1 | [Icons for the buttons that are still text](icon-buttons-and-transport-controls.md) | requested |
| 2 | [Every artist and album name should be a link](clickable-artist-album-links.md) | requested |
| 3 | [Keyboard shortcuts](keyboard-shortcuts.md) | proposed |
| 4 | [Album-level highlighting, and finding the playing track](album-highlighting-and-scroll-to-playing.md) | proposed — finishes 0.4.2 deferrals |

## What each is really about

**1 — Icons.** 0.4.2 bundled an icon font and moved the *row actions* onto it,
then stopped. The transport controls — the most-used buttons in the app — are
still four boxes reading *Prev*, *Play*, *Stop*, *Next*. All nine glyphs needed
are confirmed present upstream. The plan deliberately does **not** convert
everything: form buttons (Save, Cancel, Delete) stay as words, and the sidebar
stays text because half its entries have no conventional glyph. The sleeper item
is `<- Back`, hand-written **ten times** across ten views.

**2 — Clickable names.** The Queue already does this correctly; nine other
places don't. Most of them share one structural cause: the album tile and album
row are each a *single* button, so the artist name inside can't be a second one.
That needs a layout decision, not a find-and-replace. The audit also turned up a
genuine bug — opening a multi-disc album from **Genre detail** shows only some of
its tracks, because that call site passes `artist: None`.

**3 — Keyboard shortcuts.** `grep -rn "keyboard\|on_key\|Key::" src/` returns
nothing: there is no key handling in the app at all. The hard part isn't adding
a subscription, it's making sure `Space` pauses playback everywhere *except*
while someone is typing in one of the app's dozen text inputs.

**4 — Album highlighting + jump-to-current.** Steps D and E of
[current-song-highlighting](current-song-highlighting.md), deferred out of 0.4.2
on purpose. D's risk has since dropped, because 0.4.2's layout work already
routed every list through `widgets/song_row.rs`.

## Ordering

**2 → 1 → 4 → 3.**

Plan 2 is the one with a real bug in it and the one the user asked about first.
Plan 1 is mostly mechanical once the font is extended. Plan 4 builds on
`song_row`, which plan 2 will also have been editing. Plan 3 is last because it
is the only one whose approach is still contingent on reading the vendored iced
source — its binding table is a proposal, not a verified design.

## Considered and not planned

Recorded so they aren't rediscovered from scratch, with the reason each is not
in this round:

- **Sidebar icons.** Only useful as an icon *column* beside the labels; icon-only
  would make Outputs/Partitions/Snapcast/Log/Stats a guessing game. Wants a
  design pass, not a plan.
- **Album grid virtualisation.** CLAUDE.md records ~4 widgets per album with no
  virtualisation — roughly 3200 widgets laid out every frame on an 800-album
  library. This is a **performance** plan, not a UI one, and it should be
  written the moment anyone reports the Albums view feeling slow. Nobody has.
- **The dead `theme` config.** `AppConfig::theme` (`dark_mode`, `accent_color`)
  is persisted, has no UI, and nothing reads it — `App::theme()` returns
  `Theme::Dark` unconditionally. It is either a light-mode feature or a
  deletion, and shipping a toggle that does nothing would be worse than either.
  Needs a decision from the user before it can be planned.
- **Search sections and batch select.** Deferred from
  [library-album-identity-and-multidisc](library-album-identity-and-multidisc.md)
  Part D and still open.
- **Media-key / MPRIS support.** Genuinely wanted for a background music player
  on Linux, and much larger than in-app shortcuts — a per-platform OS
  integration. Its own plan, later.
- **macOS `.app` bundle.** Steps D/E of
  [app-icon-cross-platform](app-icon-cross-platform.md); macOS still has no dock
  icon. Packaging rather than UI, but it is the most visible remaining gap from
  0.4.2.

## Verification stance

0.4.2 ended with every visual change code-verified and none of it *seen*. That
was acceptable for correctness fixes with unit tests behind them; it is not
acceptable for a round whose entire subject is how the interface looks.

**Every plan here ends with a "how to confirm" section that requires running the
app**, and several of the decisions inside them — grid-density highlighting, the
mode buttons at minimum window width, whether more clickable regions per row
feels twitchy — are explicitly flagged as needing eyes rather than reasoning.
