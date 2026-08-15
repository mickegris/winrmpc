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
| Window position | **yes, carefully** | see the monitor caveat |
| Maximised | **yes** | a maximised app that reopens windowed is jarring |
| Last view | **maybe** | see below |
| Sidebar scroll, list scroll | no | restoring scroll into data that may have changed is worse than starting at the top |

### The monitor caveat

Restoring a position blindly is how apps end up **invisible**: saved on a
second monitor that is no longer attached, or on a display whose resolution
shrank. The restore must validate the position against the *current* displays
and fall back to centred if it doesn't fit.

iced 0.13's `window::Settings` has a `position: Position` field
(`Position::Specific(Point)` / `Centered` / `Default`). **Whether iced 0.13
exposes the current monitor list, and whether it reports window
moves/resizes back as events, both need checking against the vendored source
before this is committed to** — `window::Event::Moved`/`Resized` and
`window::get_size`/`get_position` are the things to look for. If move events
aren't available, the fallback is to read the size on exit, which needs a
close-request hook.

**This is the one open question in the plan, and it decides whether the plan is
ten lines or fifty.**

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

1. Check the vendored iced for window move/resize events and monitor
   enumeration. **The rest of the plan is contingent on this.**
2. `[window]` table in `AppConfig`, all fields defaulted.
3. `main()` reads the config for the initial size/position, validating a saved
   position against the available displays and falling back to centred.
4. Persist on change (debounced — a drag emits a resize event per frame, and
   writing the TOML on each would be pathological) or on exit.
5. Keep `min_size` as it is; a restored size smaller than the minimum should be
   clamped, not honoured.

## How to confirm

- Resize, quit, relaunch — same size.
- Move to a second monitor, quit, **unplug the monitor**, relaunch — the window
  is visible on the remaining display. This is the failure this plan exists to
  avoid, and it is the only test that matters.
- Maximise, quit, relaunch — still maximised.
- Drag-resize continuously and confirm the config file isn't being rewritten
  dozens of times a second.
- Delete the `[window]` table from `config.toml` and confirm the app starts at
  1200×800 as before.
