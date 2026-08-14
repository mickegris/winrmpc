# Keyboard shortcuts

Part of [ui-0.4.3](ui-0.4.3.md). **Not requested — proposed.**

## The finding

```
$ grep -rn "keyboard\|on_key\|Key::" src/
(nothing)
```

**The app handles no keyboard input at all.** Not space to play/pause, not a
shortcut to focus search, nothing. Every action in a music player that runs in
the background all day requires finding the window and hitting a specific
button with the mouse.

For the one control people use most — pause — that is the difference between a
keystroke and a window raise, a mouse trip to the bottom-left of the screen, and
a click on a 52×28 box.

## Why this is worth a plan rather than a one-liner

Three things make it more than "add a subscription":

1. **Text inputs must not swallow shortcuts, and shortcuts must not swallow
   typing.** The app has text inputs in Search, Settings (server host/port/name,
   cache size), Radio (station name/URL), CD (device path) and the playlist
   rename/save fields. Space in a search box means a space, not pause.
2. **iced 0.13 gives no focus-aware key handling for free.** `keyboard::on_key_press`
   is a global subscription; it fires regardless of what has focus.
3. **Some obvious shortcuts collide with navigation.** Left/right as seek versus
   left/right as list navigation is a real conflict once lists are focusable.

## What iced 0.13 actually offers

`iced::keyboard::on_key_press(|key, modifiers| -> Option<Message>)` as a
subscription, composed into `App::subscription` alongside the existing `every(500ms)`
tick. That much is straightforward.

What it does **not** offer is "is a text input focused right now". Options:

- **Track focus in `App` state.** Every `text_input` already has an
  `on_input` handler; a view could set `App::text_input_focused` on focus/blur.
  iced 0.13's `text_input` has `.on_focus`/`.on_blur`… **this needs checking
  against the vendored source before committing to it** — if those don't exist
  in 0.13, this approach doesn't work.
- **Gate on the current view.** Crude but honest: suppress single-letter
  shortcuts on views that contain text inputs (Search, Settings, Radio, CD),
  keep modifier-based ones everywhere. Requires no focus tracking.
- **Use modifiers for everything.** `Ctrl`-based shortcuts never collide with
  typing. Costs the ergonomics of a bare `Space`.

**Recommend a hybrid**: bare keys for transport, gated on "the current view has
no text input"; `Ctrl`-modified for everything global. Then `Space` works on
Now Playing, Queue, Albums and the rest — which is where you are when you want
it — and typing in Search is never interrupted.

## Proposed bindings

| Key | Action | Notes |
|---|---|---|
| `Space` | Play / pause | The one that matters. Suppressed where text inputs live. |
| `Ctrl+Right` / `Ctrl+Left` | Next / previous track | Modified, so always safe |
| `Right` / `Left` | Seek ±5s | Bare; same suppression as Space |
| `Up` / `Down` | Volume ±5 | Bare. **Conflicts with list scrolling** — see below |
| `Ctrl+F` | Focus search (navigate to Search and focus the box) | Needs `text_input::focus()` |
| `Ctrl+L` | Focus… nothing yet | Skip unless there's a use |
| `Esc` | Back (`GoBack`) | Matches the ten `<- Back` buttons |
| `Ctrl+1`…`Ctrl+9` | Jump to sidebar views | Optional; nice once the sidebar order is stable |
| `/` | Focus search | Conventional, but *is* a text character — same gating as Space |

**`Up`/`Down` for volume is the questionable one.** In a scrollable list those
keys mean scroll, and iced routes them to a focused scrollable. Safer:
`Ctrl+Up`/`Ctrl+Down` for volume, leaving bare arrows to the list. Decide when
implementing, with a list open.

## Plan

### A. Verify what iced 0.13 supports

Before writing anything, read the vendored `iced_widget-0.13.4/src/text_input.rs`
and `iced-0.13.1/src/keyboard.rs` for:

- whether `text_input` exposes focus/blur callbacks,
- whether `text_input::focus(Id)` exists as an operation (it is needed for `Ctrl+F`),
- what `on_key_press` receives for modifiers.

**This plan's binding table is contingent on that.** Everything else in
0.4.2 was sourced from the vendored crates with file:line references; this
should be too, and hasn't been yet.

### B. A `Shortcut` layer, not scattered matches

One function mapping `(Key, Modifiers, &App)` → `Option<Message>`, unit-testable
without a window. The suppression rule ("this view holds a text input") is a
single predicate on `View`, so it can be asserted directly:

```rust
#[test] fn space_does_not_play_pause_while_the_search_view_is_open()
```

That test is the whole point of extracting the layer — the failure mode here is
a shortcut firing while someone types, and it is trivially testable if the
mapping is a pure function and untestable if it is a match arm inside
`subscription`.

### C. Discoverability

A shortcut nobody knows about is dead code. Cheapest honest option: a
`?`-triggered overlay, or a short list in Settings. **Prefer Settings** — one
more section, no new overlay machinery, and it sits next to Storage which is
already the "how does this thing work" area.

## Explicitly not doing

- **Global/media-key support** (play/pause when the app isn't focused). That is
  a per-platform OS integration — MPRIS on Linux, SMTC on Windows, MPNowPlaying
  on macOS — and is a much bigger piece of work than in-app shortcuts. Worth its
  own plan later; MPRIS in particular would let winrmpc respond to a keyboard's
  media keys on Linux, which is what most people actually want. Not this.
- **Rebindable shortcuts.** Fixed set first; make them configurable only if
  someone asks.
- **Vim-style navigation.** No.

## How to confirm

- **Type a space in every text input in the app** — Search, all Settings
  fields, Radio's name/URL, CD's device path, playlist rename — and confirm
  playback does not toggle. This is the acceptance criterion; everything else
  is a bonus.
- Press `Space` on Now Playing, Queue, Albums, Artists and confirm it toggles.
- Hold `Right` and confirm seeking doesn't queue up hundreds of MPD commands —
  the 500ms status poll shares one connection mutex, and a key-repeat storm is
  exactly the kind of thing that starved it before (see CLAUDE.md's
  "Play All / Queue All"). Debounce if it does.
