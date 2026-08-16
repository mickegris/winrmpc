# Errors the user never sees

Part of [ui-0.4.3](ui-0.4.3.md). **Not requested — proposed.**

## The finding

`App::last_error: Option<String>` is written from **seven** places across
`app.rs` — failed connections, playlist operations, database updates, MPD
command failures — and rendered from exactly **one**:

```
$ grep -rn "last_error" src/ui/views/ src/ui/widgets/
(nothing)

$ grep -n "last_error" src/ui/app.rs
203, 356, 401, 421, 1308, 1344, 1416, 2258, 2279   ← writes
3391                                                ← the only read, in settings_view()
```

**So an error is only visible if the user happens to be on the Settings view
when it happens.** Save a playlist with a name MPD rejects, from the Playlists
view, and the operation fails silently: no message, no change, nothing to
suggest the app even tried. The text lands in a field that is only rendered on
a screen you are not looking at.

The Log view catches most of it — but "check the Log view" is not feedback, it
is an investigation.

## A second, smaller problem

`last_error` is also used for things that are not errors:

```rust
// app.rs:2258
self.last_error = Some("Database update started".to_string());
```

So the field is really "last status message", labelled as an error, and
rendered under a heading that says `Status:`. Whatever replaces it should
distinguish **error** from **info**, because they warrant different colours and
different dismissal behaviour.

## What good looks like here

A transient toast in a corner of the window, above the player bar:

- appears on error or notable status,
- says what failed in the user's terms ("Couldn't save playlist: name already
  exists"), not the raw MPD `ACK`,
- dismisses itself after a few seconds, or on click,
- errors persist longer than info, or until dismissed.

Deliberately *not* a modal — nothing here is worth blocking the app for, and a
music client that interrupts playback controls with a dialog is worse than one
that stays quiet.

## Implementation notes

- **iced 0.13 has no toast widget.** The overlay would be a `stack`
  (`iced::widget::stack` exists in 0.13) placing a container over the main
  content, bottom-aligned above the player bar. Worth checking the vendored
  source for `stack`'s behaviour with pointer events — a toast must not
  swallow clicks meant for what's underneath it.
- **Timed dismissal needs a subscription.** The 500ms `Tick` already exists and
  is plenty precise for a 4-second toast; a dedicated timer is unnecessary.
  Store `shown_at: Instant` and drop it when expired, on the tick the app is
  already doing.
- **A queue, not a slot.** Two failures in quick succession (a bulk enqueue
  hitting several bad URIs) should not have the second silently replace the
  first before it was read. A small `VecDeque` with a cap, showing one at a
  time, is enough.
- **Message text is the deliverable.** `MpdError` renders things like
  `ACK [50@0] {save} Playlist already exists`. That is the right thing for the
  Log and the wrong thing for a toast. Mapping the common `ACK` codes to
  sentences is most of the value of this plan, and it can be done incrementally
  — anything unmapped falls back to the raw text, which is still better than
  nothing.

## Where it should fire

Auditing the seven write sites, plus the places that currently swallow errors
entirely, is part of the work. Known candidates from CLAUDE.md:

- **`add_all` partial-album failures.** Already logged as a warning with a
  "queued a partial album" message, deliberately not swallowed — this is
  exactly a toast case: the user asked for an album and got some of it.
- **`config.save_and_log` failures** (0.4.2 added the logging; the user still
  only learns their setting vanished at restart).
- **Snapcast RPC errors** — the view has an error panel, so it may not need a
  toast; worth checking it doesn't double up.

## Not in scope

- **Replacing the Log view.** It stays; it is the record. This is the
  *notification*.
- **Retry affordances** ("Couldn't connect — Retry"). Nice, but each needs its
  own message and handler; do the surfacing first.

## How to confirm

- Trigger a failure from a view that is **not** Settings — saving a playlist
  under an existing name is the easiest — and confirm it is visible where you
  are standing.
- Confirm the toast **doesn't block the player bar** underneath it, by clicking
  through where it appears.
- Trigger several failures at once (queue an album with a bad URI in it) and
  confirm they don't stomp each other.
- Confirm an info message ("Database update started") reads as info and an
  error reads as an error.
