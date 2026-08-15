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
| 3 | [Light mode / dark mode](theme-light-and-dark.md) | requested |
| 4 | [macOS `.app` bundle](macos-app-bundle.md) | requested — finishes 0.4.2 deferrals |
| 5 | [Keyboard shortcuts](keyboard-shortcuts.md) | proposed |
| 6 | [Album-level highlighting, and finding the playing track](album-highlighting-and-scroll-to-playing.md) | proposed — finishes 0.4.2 deferrals |
| 7 | [Errors the user never sees](error-and-status-feedback.md) | proposed |
| 8 | [Window size, position, and where you left off](window-and-session-state.md) | proposed |

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

**3 — Light/dark.** Resolves config that has been dead since 0.4.0:
`theme.dark_mode` and `theme.accent_color` are persisted and **nothing reads
them**. The difficulty isn't the palette, it's that `AppColors` is 16 `const`
colours used **319 times across 27 files**, mostly in `.color(...)` calls with
no theme in scope. Also recommends deleting `accent_color` rather than
implementing it.

**4 — macOS bundle.** Steps D/E of
[app-icon-cross-platform](app-icon-cross-platform.md). macOS still has no dock
icon and ships no binary. One question the original plan left open is now
answered: it said the `icns` crate would only be worth it "if a CI job
cross-builds" — CI now runs on real macOS, so `iconutil` is available and no
extra crate is needed. The plan is blunt about Gatekeeper: an unsigned `.app`
reports itself as *damaged*, and signing needs a paid Apple account.

**5 — Keyboard shortcuts.** `grep -rn "keyboard\|on_key\|Key::" src/` returns
nothing: there is no key handling in the app at all. The hard part isn't adding
a subscription, it's making sure `Space` pauses playback everywhere *except*
while someone is typing in one of the app's dozen text inputs.

**6 — Album highlighting + jump-to-current.** Steps D and E of
[current-song-highlighting](current-song-highlighting.md), deferred out of 0.4.2
on purpose. D's risk has since dropped, because 0.4.2's layout work already
routed every list through `widgets/song_row.rs`.

**7 — Error surfacing.** `last_error` is written from seven places and rendered
from **one** — inside `settings_view()`. An error is therefore only visible if
you happen to be on the Settings screen when it happens. Everything else fails
silently, with the Log view as the only trace.

**8 — Window state.** Every launch is 1200×800 wherever the WM puts it; resize
and position never survive a restart. The smallest plan here, and the one with
the sharpest failure mode to avoid — restoring a position onto a monitor that
is no longer attached.

## Ordering

**2 → 1 → 6 → 7 → 3 → 8 → 5 → 4.**

- **2 first** — it contains a real bug (multi-disc albums opened from Genres
  show only some of their tracks), not just a missing affordance.
- **1 then 6** — both are mechanical once their foundation exists, and both
  touch `song_row`/`album_grid`, which plan 2 will already have opened.
- **7** is self-contained and makes every later plan easier to debug.
- **3 (light/dark) deliberately late.** It renames colour access in 27 files;
  doing it before 1, 2, 6 and 7 means rebasing all of them across that rename.
  Do it once the view churn has settled.
- **8 and 5** are both contingent on reading the vendored iced source (window
  events; keyboard focus). Either could move earlier once that's checked.
- **4 (macOS) last, and separable.** It touches no view code at all — it is
  packaging plus a CI job, and could equally be done first by someone with a
  Mac to test on. It is the only plan here that **cannot be verified from this
  machine**.

Nothing here has to ship as one release. Plans 1, 2, 6, 7 are a coherent 0.4.3;
3, 4, 5, 8 could each land on their own.

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
- **Search sections and batch select.** Deferred from
  [library-album-identity-and-multidisc](library-album-identity-and-multidisc.md)
  Part D and still open.
- **Media-key / MPRIS support.** Genuinely wanted for a background music player
  on Linux, and much larger than in-app shortcuts — a per-platform OS
  integration. Its own plan, later.
- **Drag-and-drop queue reordering.** The move-up/move-down arrows work; drag
  would be nicer and is a much bigger piece of iced work. Not now.

## Verification stance

0.4.2 ended with every visual change code-verified and none of it *seen*. That
was acceptable for correctness fixes with unit tests behind them; it is not
acceptable for a round whose entire subject is how the interface looks.

**Every plan here ends with a "how to confirm" section that requires running the
app**, and several of the decisions inside them — grid-density highlighting, the
mode buttons at minimum window width, whether more clickable regions per row
feels twitchy — are explicitly flagged as needing eyes rather than reasoning.
