# Icons for the buttons that are still text

Part of [ui-0.4.3](ui-0.4.3.md).

## What exists now

0.4.2 bundled an icon font (`src/ui/widgets/icon.rs`, 17 glyphs) and moved every
**row action** onto it. It stopped there. The rest of the app's controls are
still text labels, and the most-used controls in the whole application — the
transport buttons — are among them.

`src/ui/widgets/player_bar.rs:47-56`:

```rust
styled_control_btn("Prev",  Message::Previous, false),
styled_control_btn(if is_playing { "Pause" } else { "Play" }, …, true),
styled_control_btn("Stop",  Message::Stop, false),
styled_control_btn("Next",  Message::Next, false),
```

Four 52×28px boxes containing the words *Prev*, *Play*, *Stop*, *Next*. They work,
but they read as a form rather than a player, and they are the one place in a
music client where the icon vocabulary is genuinely universal — nobody needs
"Prev" spelled out next to a ⏮.

## Inventory

Everything below was found by grepping `button(text(` and the `player_bar`
helpers across `src/ui/`. Grouped by whether an icon is an improvement, which
is **not** the same question as whether an icon exists.

### A. Icons are strictly better — universal, and the label adds nothing

| Control | Where | Material Symbol | cp |
|---|---|---|---|
| Prev | `player_bar.rs:48` | `skip_previous` | `e045` |
| Play / Pause | `player_bar.rs:49` | `play_arrow` (have) / `pause` | `e034` |
| Stop | `player_bar.rs:54` | `stop` | `e047` |
| Next | `player_bar.rs:55` | `skip_next` | `e044` |
| `<- Back` (**10 occurrences**) | album, artist, genre_detail, playlist*, recently_played, outputs, partitions, … | `arrow_back` | `e5c4` |
| Vol | `player_bar.rs:75` | `volume_up` | `e050` |

All codepoints verified present in upstream Material Symbols Outlined.

`<- Back` is worth calling out separately: it is hand-written **ten times** with
its own `button(text("<- Back").size(14).color(ACCENT))` block each time. That is
a shared widget waiting to happen regardless of the icon question.

### B. Icon **plus** retained text — the icon speeds recognition, the word carries the meaning

The playback modes (`player_bar.rs:90-101, 145-151`) are the interesting case:

```rust
let repeat_text  = if status.repeat { "Repeat On" } else { "Repeat Off" };
let single_text  = match status.single { On => "Single On", Oneshot => "Single 1x", … };
let consume_text = match status.consume { On => "Consume On", … };
```

- `repeat` (`e040`) and `repeat_one` (`e041`) and `shuffle` (`e043`) are standard.
- **`single` and `consume` have no standard icon**, because they are MPD concepts.
  "Consume" — remove each track from the queue after playing it — is not something
  a glyph communicates to anyone who doesn't already know MPD.
- These are **tri-state** (`Off` / `On` / `Oneshot`), not binary. An icon alone
  cannot show `Oneshot`; today the label says "1x".

**Recommendation: keep the words, add the icons, drop the "On/Off" suffix.**
State is already carried by `mode_btn`'s background colour (accent when active),
so "Repeat On" / "Repeat Off" is saying twice what the colour says once. `Repeat`,
`Random`, `Single`, `Consume` with an icon and an accent background reads better
and takes less width — and `Single 1x` / `Consume 1x` stays available for the
oneshot state, which is the one thing the colour can't express.

### C. Leave as text — an icon would be a guess

- **Sidebar's 17 nav entries** (`sidebar.rs:52-73`). Icons for Now Playing / Queue /
  Artists / Albums / Genres are fine, but Outputs, Partitions, Snapcast, Log and
  Stats have no conventional glyph, and the sidebar is 132px wide precisely so
  labels fit. An icon **column** beside the labels is a different (and reasonable)
  design; icon-only would make half the sidebar a guessing game. Out of scope here.
- **Dialog and form buttons** — Save, Cancel, Create, Rename, Connect, Add Station,
  Set as default, Clear cache, Open folder, Play All, Queue All, Play Whole CD.
  These are commitments, and a text button that says what it commits to is better
  than a glyph. Leave them.

## Plan

### A. Extend the icon font

Add to `packaging/fonts/build-icon-font.py`'s `OUTLINE` map and to
`ui::widgets::icon`'s constants + `ALL`:

`skip_previous`, `pause`, `stop`, `skip_next`, `arrow_back`, `volume_up`,
`repeat`, `repeat_one`, `shuffle`.

Nine glyphs; the subset is ~2.6 KB today so this stays trivially small. The
existing tests (`every_glyph_constant_exists_in_the_bundled_font`,
`the_bundled_font_carries_no_glyph_the_ui_cannot_name`) enforce both directions,
so a constant without a glyph or a glyph without a constant fails the build.

**`every_glyph_advances_exactly_one_em` must keep passing** — `song_row::ACTION_BTN_WIDTH`
is derived from the assumption that glyphs are square. All nine candidates come
from the same family at the same optical size, so this should hold; the test is
what proves it rather than hoping.

### B. Transport controls

Replace the labels in `styled_control_btn`. Keep the existing 52×28 button
geometry and the primary/secondary colour split — this is a label swap, not a
redesign of the bar.

**Add tooltips** (`icon_btn_tip` already exists). Play/Pause/Next are obvious;
Stop is worth a tooltip because *stop* vs *pause* is a real distinction in MPD
(stop resets the position) that the glyph doesn't convey.

Consider whether Play/Pause should be visually larger than the other three, as
it is the one control anyone reaches for repeatedly. It already gets the accent
background; that may be enough.

### C. A shared back button

Replace all ten hand-rolled `<- Back` blocks with one widget in
`ui::widgets::link` — `back_button()` — using `arrow_back` plus the word "Back".
This is the single highest-value item in the plan by lines-removed, and it means
the back affordance can never drift between views again.

### D. Playback-mode buttons

Icon + shortened label per section B. `mode_btn` takes a `&str`; it will need a
glyph parameter. The tri-state views (`Single`, `Consume`) keep their `1x`
suffix for `Oneshot`.

Repeat is the one that can use two glyphs: `repeat` normally, `repeat_one` when
`single` is also on — that pairing is what a user actually means by "repeat this
track", and showing it is free once both glyphs are bundled.

### E. Volume

`text("Vol")` → `volume_up`. Consider making it a **mute toggle** while there:
MPD has no mute, but `setvol 0` with the previous volume restored on the next
click is the conventional behaviour, and a speaker icon that does nothing when
clicked is a worse affordance than the text was. If mute is not implemented,
leave the icon non-interactive and say so in the code.

## Explicitly not doing

- **Sidebar icons.** Half the entries have no conventional glyph (section C).
- **Icon-only form buttons.** Save/Cancel/Delete stay as words.
- **Replacing the `+ Add to Playlist` text link in Now Playing.** CLAUDE.md
  already records it as the clearest control of the set; it has room for words.

## How to confirm

The font work is testable (`cargo test`), the rest is not:

- **Before/after screenshot of the player bar** at the 1000px minimum window
  width — the mode buttons are the widest thing in the bar and the point of
  dropping "On"/"Off" is that they stop competing with the transport controls.
- **Hover every transport button** and confirm the tooltip appears and doesn't
  clip at the window's bottom edge (`tooltip::Position::Top` is what the row
  actions use; the player bar is at the *bottom* of the window, so Top is right
  here too — but this needs looking at, not reasoning about).
- **Check the Oneshot states render** — set `single` and `consume` to `1x` via
  the buttons and confirm the label still distinguishes them from `On`.
