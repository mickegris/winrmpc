# Light mode / dark mode

Part of [ui-0.4.3](ui-0.4.3.md).

## What exists now

Three things that don't meet:

1. **`AppConfig::theme`** — `ThemeConfig { dark_mode: bool, accent_color: String }`,
   persisted to `config.toml`, defaulting to `dark_mode: true` and
   `"#4fc3f7"`. **Nothing reads either field.**
2. **`App::theme()`** (`app.rs:362`) — returns `Theme::Dark` unconditionally.
   This is what iced's *built-in* widgets style themselves from: `pick_list`,
   `slider`, `text_input`, `scrollable`, default `button`s, the menu popup.
3. **`AppColors`** (`ui/theme/colors.rs`) — **16 `const Color`s**, referenced
   **319 times across 27 files**. This is what everything the app draws itself
   styles from.

CLAUDE.md has recorded this as "dead config" since 0.4.0, with the note that a
toggle doing nothing would be worse than no toggle. That's still right, so this
plan either makes it real or deletes the fields.

## The actual difficulty

It is not the palette. It is that **`AppColors::TEXT_PRIMARY` is a `const`, and
most uses are direct `.color(...)` calls in view code with no theme in scope**:

```rust
text(song.display_title()).size(13).color(AppColors::TEXT_PRIMARY)
```

iced hands `&Theme` to *style closures* (`|theme, status| …`), and the app's
closures all ignore it (`|_t: &iced::Theme, status|`). But a plain `.color()`
takes a `Color`, not a closure. So there is no thread from "the user picked
light" to those 319 call sites without changing how colours are fetched.

### Three ways to bridge it

**A. Mode-indexed lookup functions.** `AppColors::TEXT_PRIMARY` becomes
`colors::text_primary()`, reading a process-global mode:

```rust
static MODE: AtomicU8 = AtomicU8::new(DARK);
const PALETTES: [Palette; 2] = [DARK_PALETTE, LIGHT_PALETTE];

pub fn text_primary() -> Color { PALETTES[MODE.load(Relaxed) as usize].text_primary }
```

- *For*: 319 mechanical edits, **zero signature changes**, no borrow plumbing
  through 20+ view functions. Lock-free — an `AtomicU8` load per colour is
  nothing next to laying out the widget that uses it.
- *Against*: process-global mutable state, which is exactly the kind of thing
  that makes tests order-dependent. Mitigate by having the tests set the mode
  explicitly rather than assuming a default.

**B. Thread `&Palette` through every view function.** Honest, no globals, and a
signature change on every `views::*::view()` plus every helper plus every call
site in `app.rs`. Large, and it makes every future view take a parameter it
mostly forwards.

**C. Put the colours in `iced::Theme` and read them in style closures.** The
idiomatic iced answer, but it only reaches the closures — the `.color()` calls
would each have to become `.style(|theme| …)`. That's a *bigger* rewrite than A
at every one of those sites, for the same result.

**Recommend A.** It is the smallest honest change, and the global is written
once at startup and once per toggle.

## The other half: iced's own widgets

Swapping `AppColors` alone gives a light app with **dark dropdowns, dark text
inputs and dark sliders**, because those come from `App::theme()`. So
`App::theme()` must return `Theme::Light` / `Theme::Dark` to match.

**Checked: `Theme::custom(String, Palette)` exists**
(`iced_core-0.13.2/src/theme.rs:88`), and `Palette`
(`theme/palette.rs:11`) is exactly five colours — `background`, `text`,
`primary`, `success`, `danger`. Every one of them has an obvious counterpart in
`AppColors` (`BG_PRIMARY`, `TEXT_PRIMARY`, `ACCENT`, `SUCCESS`, `ERROR`).

**So build both themes with `Theme::custom` from the app's own palette** rather
than hoping iced's stock Light sits well beside a hand-tuned one. Both halves
then derive from one source and cannot drift — the same property that makes the
icon font and the app icon testable.

## Designing the light palette

The 16 colours, by use count: `TEXT_MUTED` (83), `TEXT_PRIMARY` (73), `ACCENT`
(43), `TEXT_SECONDARY` (34), `BG_SECONDARY` (19), `ROW_ODD`/`ROW_EVEN` (10 each),
`ERROR` (10), `BG_PRIMARY` (8), `SUCCESS`/`ROW_PLAYING`/`BORDER`/`BG_TERTIARY`/
`BG_HOVER` (6 each), `WARNING` (5), `TEXT_DISABLED` (2).

Not a mechanical inversion. Three specifics:

- **`ACCENT` is `#4fc3f7`**, a light cyan chosen against a near-black
  background. On white it is close to illegible. Light mode needs a darker,
  more saturated accent — and since `ACCENT` is also the *playing track's* text
  colour, this is a legibility issue, not a taste one.
- **The row colours have a relationship, not just values.** `ROW_EVEN`,
  `ROW_ODD`, `ROW_PLAYING` and `BG_HOVER` must stay mutually distinguishable —
  `song_row`'s tests already assert exactly this. **Those tests currently check
  one palette and must be made to run against both**, which is the cheapest
  possible guard against shipping a light mode where the playing row is
  invisible.
- **`WARNING` (`#f2bf40`) and `SUCCESS` on white** both lose contrast. Semantic
  colours usually need darkening in light mode.

## What to do about `accent_color`

The config carries a hex string nobody reads. Two coherent options:

- **Drop the field**, keep `dark_mode`. Simplest; the accent becomes part of
  each palette.
- **Honour it**, parsing the hex and overriding the palette's accent. Then it
  needs validation, a Settings colour input, and a decision about what a custom
  accent means in the *other* mode.

**Recommend dropping it.** It has never worked, nobody has asked, and a custom
accent that has to remain legible on both a near-black and a near-white
background is a real design constraint for a feature nobody requested. Removing
a `#[serde(default)]` field is backward-compatible — old files keep parsing.

## Follow the system?

A third setting — *Light / Dark / System* — is what people expect. It needs OS
detection, which iced 0.13 does not provide; the `dark-light` crate does, on all
three platforms. Live switching when the OS setting changes needs a watcher
subscription and is a further step beyond reading it once at startup.

**Recommend shipping Light/Dark first** and adding System only if wanted. Reading
it once at startup is a reasonable middle ground.

## Plan

1. Turn `AppColors` into a `Palette` struct plus two constants and the
   mode-indexed accessors (option A). Mechanical rename across 27 files.
2. `App::theme()` returns `Theme::custom` built from the active palette, so
   iced's own widgets and the app's hand-drawn ones share one source.
3. Design the light palette, honouring the three specifics above.
4. Make the `song_row` colour-relationship tests run against **both** palettes.
5. Settings → Appearance: a Light/Dark toggle writing `theme.dark_mode`,
   applied immediately (no restart) and persisted via `save_and_log`.
6. Delete `accent_color`.

Steps 1–2 are a refactor with no user-visible change and can land alone; that's
the natural first commit, since it's the one that touches 27 files.

## How to confirm

- Toggle in Settings and watch **every** view — the failure mode is one view
  someone forgot, and 27 files means there will be one. Particularly: the Log
  view's monospace rows, Snapcast's sliders, the Settings inputs themselves, the
  pick_lists (replay gain, server picker), and the placeholder blocks drawn for
  missing album art.
- Confirm the **playing row is still obvious** in light mode, in every list.
- Confirm the choice **survives a restart**.
- Check the app against a light OS *and* a dark OS desktop — a light app with a
  dark window frame is fine, the reverse can look broken.
