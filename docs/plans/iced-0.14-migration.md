# Plan: Migrate winrmpc from iced 0.13 → 0.14

Target: bump `iced = "0.13"` → `"0.14"` (released 2025-12-07) and get a clean
`cargo build --release` + visually-verified app, with no feature regressions.

Source: https://docs.rs/crate/iced/latest/source/CHANGELOG.md

---

## 0. Good news first (what we DON'T have to do)

The 0.13→0.14 jump is mostly additive for an app like ours:

- **We're already on the `Task` API** (0.13). The big `Command → Task` rename that hurt 0.12→0.13 migrators is behind us. We use `Task::perform`, `Task::batch`, `Task::none`, `scrollable::snap_to(...) -> Task`. All survive.
- **We already pin `image = "0.25"`** (`Cargo.toml:14`), which is exactly what iced 0.14 upgraded to — no dual-version of the `image` crate.
- **We implement no custom `Widget`/`Overlay`.** The headline breaking changes — "mutable `Widget` methods" (#3038), `Widget::update` takes `&Event` (#2781), `is_over` removed from `Overlay` (#2921) — **do not touch us**. We only compose built-in widgets.
- **We don't use the `color!` macro** (grep clean), so its removed shorthand (#2592) is irrelevant. Colors come from `AppColors` constants + `.into()`.

So this is expected to be a *small* migration dominated by a toolchain bump and a few signature checks, not a rewrite.

---

## 1. Toolchain prerequisite (the one hard gate)

iced 0.14 **"Updated to Rust 2024"** (#2809). Crates using the Rust 2024 edition require **rustc ≥ 1.85**. Our crate can stay on `edition = "2021"`, but the *compiler* must be new enough to build the 0.14 dependency.

- [ ] `rustc --version` → ensure ≥ 1.85 (`rustup update stable` if not).
- [ ] No need to change our own `edition = "2021"` unless we want 2024 features.

If the toolchain is too old, nothing else compiles — do this first.

---

## 2. Surfaces in our code that touch iced API (audit list)

These are the only places 0.14 could break us. Each must be re-checked against 0.14 docs during the bump:

| Area | Our usage | File(s) | Risk |
|------|-----------|---------|------|
| **App entry point** | `iced::application("winrmpc", App::update, App::view).subscription(...).theme(...).window(...).run_with(App::new)` | `main.rs:38` | **Medium.** The Program/Daemon abstraction was reworked (#2331, #2469). The `iced::application(...)` builder is expected to remain, but the exact builder signature / `run_with` must be verified. Most likely compiles unchanged. |
| **Window settings** | `iced::window::Settings { size, icon, ..Default::default() }` | `main.rs:41` | **Low.** New fields added (`maximized`, `vsync`, `transparent`, `CornerPreference`, …) but all behind `..Default::default()`. Compiles as-is. |
| **Scrollable programmatic scroll** | `scrollable::Id::unique()`, `scrollable::snap_to(id, RelativeOffset{..})`, `.id(scroll_id)` | `app.rs:90,167,1671`, `now_playing.rs:23,319` | **Low–Medium.** Scrollable got "smart scrollbars", `auto_scroll` (#2973), and `Background`-based styling (#3127). `Id`/`snap_to`/`RelativeOffset` should persist. **Bonus:** `auto_scroll` may let us *delete* our manual lyric auto-scroll hack. |
| **Widget style closures** | `|_t: &iced::Theme, status: button::Status| button::Style {..}` and `|_t: &iced::Theme| container::Style {..}` across views/widgets | `link.rs`, `now_playing.rs`, `album.rs`, `browser.rs`, `search.rs`, `log.rs`, `player_bar.rs`, `sidebar.rs`, etc. | **Medium.** Closure-based styling is the same model 0.13 introduced, but `button::Style` / `container::Style` field sets can shift between minor versions. Expect to adjust a few struct literals if fields were added/renamed. |
| **Fonts** | `Font::with_name("Segoe UI Symbol")`, `Font::MONOSPACE` | `link.rs:36`, `log.rs:81` | **Low.** Font API stable; `style` attribute added (#2041) is additive. |
| **Core types** | `Element`, `Length::{Fill,FillPortion,Fixed}`, `Color`, `Alignment`, `Border`, `Shadow`, `Padding`, `Background` | everywhere | **Low**, but see §3 layout note. |

---

## 3. Behavioral change to watch: layout "Shrink over Fill" (#3045)

0.14 **"Prioritized `Shrink` over `Fill` in layout logic."** We lean heavily on
`Length::Fill` / `FillPortion` (two-column Now Playing, sidebars, scrollables,
row spacers). This is the **highest visual-regression risk** in the whole
migration — nothing fails to compile, but spacing/proportions can shift.

Mandatory manual visual pass after it builds (see §6 checklist), especially:
- Now Playing two-column split (art/info `FillPortion(3)` vs lyrics `FillPortion(2)`).
- The `Space::with_height(Length::Fill)` pins (recently-played bottom alignment).
- Sidebar width vs main content.
- Scrollable fill in lyrics/queue/search/log lists.

---

## 4. Dependency / transitive notes

- `wgpu` → 27.0, `cosmic-text` → 0.15 (transitive via iced). Pulled automatically; just a longer first rebuild. No app code change.
- Default `PowerPreference` is now **`HighPerformance`** in `iced_wgpu` (#2813). On laptops this may prefer the discrete GPU (more battery). Note only; no action unless users complain.
- Removed deps (`once_cell`, `winapi`, `palette`) are internal to iced — no effect on us.
- `winres` build-dep and the icon-embedding path are unrelated to iced; unaffected.

---

## 5. Step-by-step

1. **Branch:** `git checkout -b chore/iced-0.14`.
2. **Toolchain:** verify `rustc ≥ 1.85` (§1).
3. **Bump:** `iced = { version = "0.14", features = ["tokio", "image", "svg", "advanced"] }` in `Cargo.toml`. (Confirm all four features still exist in 0.14; `tokio`/`image`/`svg`/`advanced` are expected to persist.)
4. **`cargo update`** to pull the new tree; **`cargo check`**.
5. **Fix compile errors iteratively**, expected to cluster in:
   - the `iced::application(...)` entry chain (`main.rs`),
   - `button::Style` / `container::Style` struct literals if fields changed,
   - any scrollable styling/builder rename.
6. **`cargo build --release`** clean.
7. **`cargo test`** — all 45 should pass untouched (tests are pure-logic, no iced types).
8. **Manual visual QA** (§6).
9. **Opportunistic cleanup (optional, separate commit):** if `scrollable::auto_scroll` (#2973) covers our needs, replace the manual `lyrics_autoscroll` + `snap_to` machinery in `app.rs`/`now_playing.rs` with it and drop `LYRIC_SYNC_OFFSET` plumbing where possible.
10. Ship via the normal **ship**/**release** flow.

---

## 6. Manual QA checklist (post-build, the part tests can't cover)

- [ ] App launches; window icon present.
- [ ] Now Playing: art + info layout correct **with lyrics shown** (two-column) **and hidden** (left-aligned). Proportions look right (Shrink-over-Fill check).
- [ ] Lyrics: loading → synced highlight → auto-scroll follows playback; scrollbar at far-right edge; "No lyrics"/"Instrumental" states.
- [ ] Recently Played strip renders, art loads, bottom-pinned.
- [ ] Play (▶) / queue (+) glyphs render (Segoe UI Symbol) in album/search/browser; clicks work.
- [ ] Queue, Search, Browser, Artists/Albums/Genres, Radio, CD, Outputs, Partitions, Settings, Log all render and scroll.
- [ ] Player bar: seek slider, volume, transport buttons.
- [ ] Log view MPD-only filter toggle + copy.
- [ ] Partition switch + persistence still works.

---

## 7. Risks & rollback

- **Risk:** entry-point/Program API signature changed more than expected → contained to `main.rs`; consult 0.14 `iced::application` docs/examples.
- **Risk:** subtle layout shifts (§3) → caught by §6 visual pass, fixed with explicit `Length`/`width`/`height` where Shrink now wins.
- **Risk:** a style struct gained a non-defaulted field → add the field to our literals.
- **Rollback:** the bump lives on its own branch; if 0.14 misbehaves, abandon the branch — `main` stays on 0.13. Low blast radius.

## 8. Effort estimate
Likely **one focused session**: a toolchain bump, a handful of struct-literal/entry fixes, and a careful visual QA pass. The long pole is the manual layout verification, not the code.
