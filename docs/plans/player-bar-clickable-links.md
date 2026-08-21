# Plan: the player bar's artist and album are links

Status: **implemented** (0.4.4). Finishes
[`clickable-artist-album-links.md`](clickable-artist-album-links.md), whose
rule is stated in CLAUDE.md as *"Every artist and album name the app renders
is a link to that artist or album"* — and which the bottom-left of the window
still isn't.

## What's there now

`src/ui/widgets/player_bar.rs:32`:

```rust
column![
    text(song.display_title()).size(14).color(...),
    text(format!("{} - {}", song.display_artist(), song.display_album()))
        .size(12)
        .color(AppColors::text_secondary()),
]
.width(250)
```

One `text` holding both names and the separator. It is the last plain-text
artist/album pair in the app — Now Playing's own info column
(`views/now_playing.rs:72`) has used `link::artist_link` / `link::album_link`
since 0.4.3, so the same two names are a link three inches up the screen and
dead text at the bottom.

## The change

```rust
column![
    text(song.display_title()).size(14).color(AppColors::text_primary()),
    row![
        link::artist_link(song.display_artist(), 12),
        text(" – ").size(12).color(AppColors::text_muted()),
        link::album_link(
            song.display_album(),
            Some(song.display_album_artist()),
            12,
        ),
    ]
    .align_y(Alignment::Center),
]
```

Three points, each of which the existing helpers already decide for us:

- **The separator has to be its own widget.** A link is a `button`, and a
  button can't share a `text` with the neighbouring name — the same reason
  `icon.rs` insists icon and label are separate widgets.
- **`album_link` gets the *album* artist**, not the track artist, matching
  `now_playing.rs`. That is what makes the target the same album page the
  cover tile opens, and `album_message` strips a placeholder artist to `None`
  on its own.
- **Placeholders stay inert.** `is_real_name` already degrades
  "Unknown Artist"/"Unknown Album" to muted text, which is what keeps a radio
  stream's junk tags from offering a link to an empty page.

## The layout risk, and how it's handled

The slot is a fixed `width(250)` and the bar has **no fixed height** — it is
`container(column![progress, row![...]])`, so its height follows its tallest
child.

Today a long "Artist - Album" is one `text`, and iced word-wraps it to a
second line: the bar silently grows taller. With three sibling widgets in a
`row`, wrapping is no longer available — a row overflows instead, and iced
widgets don't clip to their parent, so a long pair would **draw over the
transport buttons**. That is a worse failure than the one being replaced.

Fix: wrap the row in `container(...).width(250).clip(true)`
(`iced_widget-0.13.4/src/container.rs:209`). A long pair truncates at the slot
edge instead of overlapping, and the bar keeps a constant height — which is
also an improvement on today's behaviour, where a two-line wrap makes the
whole bar jump by ~15px when the track changes.

No change to `link.rs`. Controlling wrapping inside the link would mean a new
variant of both helpers; clipping at the container is one line and keeps the
two call sites identical to every other one in the app.

## Tests

The player bar has no view-level test harness (no view module does — they're
`Element` builders). What is testable and worth pinning:

- `link::album_message` already has coverage for the placeholder rule; nothing
  new needed there.
- `the_song_info_slot_is_wide_enough_for_a_name_pair` — `250` is now
  `SONG_INFO_WIDTH`, set on both the column and the clip container, and
  bounded on both sides: too narrow and the pair is always truncated, too wide
  and it crowds the transport controls at the minimum window width.

Verification is manual, and specific: play a track with a long artist and a
long album title and confirm the pair truncates rather than colliding with the
Previous button, and that the bar's height doesn't change when the track does.
