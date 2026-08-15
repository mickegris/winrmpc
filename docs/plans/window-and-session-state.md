# Window size, position, and where you left off

Part of [ui-0.4.3](ui-0.4.3.md). **Not requested — proposed.** The smallest
plan in the round.

## The finding

`main.rs:56-64`:

```rust
size: iced::Size::new(1200.0, 800.0),
min_size: Some(iced::Size::new(1000.0, 700.0)),
```

**Every launch is 1200×800, wherever the window manager decides to put it.**
Resize it, move it to a second monitor, maximise it — none of that survives a
restart. For an application people leave running and relaunch daily, that is a
small irritation repeated indefinitely.

`View::default()` is `NowPlaying` (`message.rs:298`), so the app also always
starts on Now Playing regardless of where you were. That one is arguably
correct — Now Playing is a reasonable home — so it's listed here as a question
rather than a defect.

## What to persist

| State | Persist? | Why |
|---|---|---|
| Window size | **yes** | the main irritation |
| Window position | **no** — see below | can strand the window offscreen |
| Maximised | **yes** | a maximised app that reopens windowed is jarring |
| Last view | **maybe** | see below |
| Sidebar scroll, list scroll | no | restoring scroll into data that may have changed is worse than starting at the top |

### The monitor caveat

Restoring a position blindly is how apps end up **invisible**: saved on a
second monitor that is no longer attached, or on a display whose resolution
shrank. The restore would have to validate the position against the *current*
displays and fall back to centred if it doesn't fit.

**Checked against the vendored source. The reporting half is fully available;
the validation half is not.**

Present in `iced_runtime-0.13.2/src/window.rs`:

| API | line | use |
|---|---|---|
| `events() -> Subscription<(Id, Event)>` | 180 | `Moved` / `Resized` / `CloseRequested` |
| `resize_events() -> Subscription<(Id, Size)>` | 213 | size changes directly |
| `close_requests() -> Subscription<Id>` | 224 | save on exit |
| `get_size(Id) -> Task<Size>` | 273 | query |
| `get_maximized(Id) -> Task<bool>` | 280 | query |
| `get_position(Id) -> Task<Option<Point>>` | 304 | query |

Plus `Position::Specific(Point)` in `window::Settings` for initial placement.

**But there is no monitor enumeration anywhere in iced 0.13.** So a saved
position cannot be validated against the displays that exist at restore time,
and the failure mode is unrecoverable *from inside the app*: the window opens
on a monitor that is no longer attached, is invisible, and the only fix is
hand-editing `config.toml`.

**Scope call: restore size and maximised state, not position.** Size is the
actual irritation; position is the part that can strand the window. This is
recorded as a deliberate limitation rather than an oversight, and it can be
revisited if iced gains a monitor API.

### Last view

Restoring the last view has one bad case: the app reopens on a detail view for
an album that no longer exists, or on Settings because that's where you were
when you quit. Restoring only the *top-level* views (Now Playing, Queue,
Albums, …) and never a detail view avoids that, at the cost of a rule to
explain. **Recommend leaving this out of the first pass** and doing size and
position, which have no such ambiguity.

## Where it goes

`AppConfig`, alongside the rest — a `[window]` table with
`width`/`height`/`x`/`y`/`maximized`, all `#[serde(default)]` so existing
config files keep parsing (the rule CLAUDE.md already documents).

**Not the cache DB.** This is user preference, not re-derivable data — same
reasoning that put `recent_albums` in the config and `recently_played` in redb.

One wrinkle: `AppConfig::load()` happens inside `App::new`, but
`window_settings()` is called **before** the app is constructed, in `main()`.
So the config has to be read (or read twice) in `main` to size the window.
Reading it twice is harmless — it is a small TOML file read once at startup —
and much simpler than restructuring startup.

## Plan

1. ~~Check the vendored iced.~~ Done — see above. Reporting is available;
   monitor enumeration is not, which is what cuts position from scope.
2. `[window]` table in `AppConfig`, all fields defaulted.
3. `main()` reads the config for the initial size. **Position is not restored**
   (see above); the window stays `Position::default()`.
4. Persist on change (debounced — a drag emits a resize event per frame, and
   writing the TOML on each would be pathological) or on exit.
5. Keep `min_size` as it is; a restored size smaller than the minimum should be
   clamped, not honoured.

## How to confirm

- Resize, quit, relaunch — same size.
- Move to a second monitor, quit, **unplug the monitor**, relaunch — the window
  is visible. (Trivially true now that position isn't restored; still worth
  checking once, because it is the failure this scope call exists to avoid.)
- Maximise, quit, relaunch — still maximised.
- Drag-resize continuously and confirm the config file isn't being rewritten
  dozens of times a second.
- Delete the `[window]` table from `config.toml` and confirm the app starts at
  1200×800 as before.
