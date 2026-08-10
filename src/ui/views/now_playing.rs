use crate::mpd::types::*;
use crate::ui::message::{Message, View};
use crate::ui::theme::AppColors;
use crate::ui::widgets::link::{icon_btn, link, link_accent};
use iced::widget::{button, column, container, image, pick_list, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;

const REPLAY_GAIN_MODES: [&str; 4] = ["off", "track", "album", "auto"];

/// Seconds to delay synced-lyric highlighting. LRCLIB timestamps tend to mark
/// when a line *starts* slightly early relative to the audible vocal, so we
/// hold each line a touch longer before advancing.
pub(crate) const LYRIC_SYNC_OFFSET: f64 = 0.5;

pub fn view<'a>(
    current_song: &'a Option<Song>,
    status: &'a Status,
    art_data: Option<&'a iced::widget::image::Handle>,
    next_song: Option<&'a Song>,
    recent_albums: &'a [RecentAlbum],
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
    lyrics: Option<Option<&'a crate::lyrics::Lyrics>>,
    show_lyrics: bool,
    lyrics_scroll_id: iced::widget::scrollable::Id,
    playing_from: Option<&'a str>,
    replay_gain_mode: Option<&'a str>,
) -> Element<'a, Message> {
    // Toggle row is placed first in ALL branches so the outer column has a
    // stable skeleton regardless of current_song state. This prevents iced's
    // positional widget-state diffing from reusing the centered "None" layout
    // on the "Some" branches after a momentary None blip.
    let crossfade_secs = status.crossfade.unwrap_or(0);
    let crossfade_control = row![
        text("Crossfade").size(11).color(AppColors::TEXT_MUTED),
        icon_btn("-", Message::SetCrossfade(crossfade_secs.saturating_sub(1))),
        text(format!("{crossfade_secs}s")).size(12).color(AppColors::TEXT_PRIMARY),
        icon_btn("+", Message::SetCrossfade(crossfade_secs + 1)),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    let rg_picker = pick_list(
        REPLAY_GAIN_MODES.to_vec(),
        replay_gain_mode,
        |m: &str| Message::SetReplayGainMode(m.to_string()),
    )
    .text_size(11)
    .padding([2, 6]);

    let toggle_row = row![
        link("Outputs", 12, Message::NavigateTo(View::Outputs)),
        Space::with_width(12),
        link("Partitions", 12, Message::NavigateTo(View::Partitions)),
        Space::with_width(12),
        link("\u{1F551} History", 12, Message::NavigateTo(View::RecentlyPlayed)),
        Space::with_width(20),
        crossfade_control,
        Space::with_width(16),
        rg_picker,
        Space::with_width(Length::Fill),
        lyrics_toggle(show_lyrics),
    ]
    .align_y(Alignment::Center);

    let content: Element<'a, Message> = match current_song {
        Some(song) => {
            let art_widget: Element<'a, Message> = match art_data {
                Some(handle) => image(handle.clone())
                    .width(300)
                    .height(300)
                    .into(),
                None => container(text("").size(1))
                    .width(300)
                    .height(300)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(AppColors::BG_PRIMARY.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into(),
            };

            // --- info column (right of art) ---
            let tech_line = format_tech_line(song, status);
            let meta_line = format_meta_line(song);

            let mut info_items: Vec<Element<'a, Message>> = vec![
                text(song.display_title())
                    .size(28)
                    .color(AppColors::TEXT_PRIMARY)
                    .into(),
                Space::with_height(8).into(),
                // Clickable artist
                link_accent(
                    song.display_artist().to_string(),
                    20,
                    Message::ArtistSelected(song.display_artist().to_string()),
                ),
                Space::with_height(4).into(),
                // Clickable album
                link(
                    song.display_album().to_string(),
                    18,
                    Message::AlbumSelected(
                        song.display_album().to_string(),
                        Some(song.display_album_artist().to_string()),
                    ),
                ),
                Space::with_height(12).into(),
            ];

            if let Some(name) = playing_from {
                info_items.push(link(
                    format!("▤ Playing from {name}"),
                    13,
                    Message::PlaylistSelected(name.to_string()),
                ));
                info_items.push(Space::with_height(6).into());
            }
            info_items.push(link(
                "+ Add to Playlist",
                12,
                Message::OpenAddToPlaylist(vec![song.file.clone()]),
            ));
            info_items.push(Space::with_height(10).into());
            if !tech_line.is_empty() {
                info_items.push(
                    text(tech_line)
                        .size(13)
                        .color(AppColors::TEXT_MUTED)
                        .into(),
                );
                info_items.push(Space::with_height(3).into());
            }
            if !meta_line.is_empty() {
                info_items.push(
                    text(meta_line)
                        .size(13)
                        .color(AppColors::TEXT_MUTED)
                        .into(),
                );
            }

            // --- Up Next, nested at the bottom of the info column ---
            if let Some(next) = next_song {
                if let Some(pos) = next.pos {
                    info_items.push(Space::with_height(24).into());
                    info_items.push(
                        text("Up Next").size(12).color(AppColors::TEXT_MUTED).into(),
                    );
                    info_items.push(Space::with_height(5).into());
                    info_items.push(
                        button(
                            row![
                                text("▶  ").size(13).color(AppColors::ACCENT),
                                text(next.display_title())
                                    .size(14)
                                    .color(AppColors::TEXT_PRIMARY),
                                text("  —  ").size(13).color(AppColors::TEXT_MUTED),
                                text(next.display_artist())
                                    .size(13)
                                    .color(AppColors::TEXT_SECONDARY),
                            ]
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::QueuePlay(pos))
                        .padding(0)
                        .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                            background: None,
                            text_color: AppColors::TEXT_PRIMARY,
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        })
                        .into(),
                    );
                }
            }

            let info = column(info_items).spacing(2);

            // art + info side by side
            let top_row = row![art_widget, Space::with_width(30), info]
                .align_y(Alignment::Start);

            // --- Recently Played: bottom-left, bigger, last 5 ---
            let current_key = song.art_key();
            let visible_recents: Vec<&RecentAlbum> = recent_albums
                .iter()
                .filter(|r| format!("{}\x1f{}", r.artist, r.album) != current_key)
                .take(5)
                .collect();

            let recents_section: Element<'a, Message> = if visible_recents.is_empty() {
                Space::with_height(0).into()
            } else {
                let thumbs: Vec<Element<'a, Message>> = visible_recents
                    .into_iter()
                    .map(|recent| recent_thumb(recent, art_handles))
                    .collect();

                column![
                    text("Recently Played")
                        .size(14)
                        .color(AppColors::TEXT_SECONDARY),
                    Space::with_height(10),
                    row(thumbs).spacing(14),
                ]
                .into()
            };

            if show_lyrics {
                // Two-column: left = art/info/recents (left-aligned), right = lyrics.
                let elapsed = status.elapsed.map(|d| d.as_secs_f64()).unwrap_or(0.0);
                let left_col = column![
                    top_row,
                    Space::with_height(Length::Fill),
                    recents_section,
                ]
                .width(Length::FillPortion(3))
                .height(Length::Fill);

                let right_col = lyrics_column(lyrics, elapsed, lyrics_scroll_id);

                column![
                    toggle_row,
                    Space::with_height(8),
                    row![left_col, Space::with_width(24), right_col]
                        .width(Length::Fill)
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            } else {
                // Single-column: art on the left, song info to its right
                // (left-aligned, consistent with the lyrics-shown layout),
                // recents pinned bottom.
                column![
                    toggle_row,
                    Space::with_height(8),
                    top_row,
                    Space::with_height(Length::Fill),
                    recents_section,
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        }
        None => {
            // Same column skeleton as the Some branches; "Nothing playing"
            // is centered via a nested container rather than align_x on the
            // outer column, avoiding stale positional state after a None blip.
            let placeholder: Element<'a, Message> = container(
                column![
                    text("Nothing playing")
                        .size(24)
                        .color(AppColors::TEXT_MUTED),
                    Space::with_height(8),
                    text("Add songs to the queue and press play")
                        .size(16)
                        .color(AppColors::TEXT_MUTED),
                ]
                .align_x(Alignment::Center),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

            column![toggle_row, Space::with_height(8), placeholder]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(30)
        .into()
}

/// One recently-played album thumbnail (120px) with its name beneath; clickable.
fn recent_thumb<'a>(
    recent: &'a RecentAlbum,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
) -> Element<'a, Message> {
    let key = format!("{}\x1f{}", recent.artist, recent.album);
    let thumb_art: Element<'a, Message> = match art_handles.get(&key) {
        Some(handle) => image(handle.clone()).width(120).height(120).into(),
        None => container(text("").size(1))
            .width(120)
            .height(120)
            .style(|_t: &iced::Theme| container::Style {
                background: Some(AppColors::BG_SECONDARY.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
    };

    let label = truncate(&recent.album, 18);
    let album_name = recent.album.clone();

    button(
        column![
            thumb_art,
            Space::with_height(6),
            text(label).size(13).color(AppColors::TEXT_SECONDARY),
        ]
        .align_x(Alignment::Center)
        .width(120),
    )
    .on_press(Message::AlbumSelected(album_name, Some(recent.artist.clone())))
    .padding(4)
    .style(|_t: &iced::Theme, status: button::Status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(AppColors::BG_HOVER.into())
            }
            _ => None,
        };
        button::Style {
            background: bg,
            text_color: AppColors::TEXT_SECONDARY,
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            shadow: iced::Shadow::default(),
        }
    })
    .into()
}

/// Small "Show lyrics" / "Hide lyrics" toggle button.
fn lyrics_toggle<'a>(show: bool) -> Element<'a, Message> {
    let label = if show { "Hide lyrics" } else { "Show lyrics" };
    button(text(label).size(12))
        .on_press(Message::ToggleLyrics)
        .padding([4, 12])
        .style(|_t: &iced::Theme, status: button::Status| {
            let (bg, fg) = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    (Some(AppColors::BG_HOVER.into()), AppColors::ACCENT)
                }
                _ => (Some(AppColors::BG_SECONDARY.into()), AppColors::TEXT_SECONDARY),
            };
            button::Style {
                background: bg,
                text_color: fg,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                shadow: iced::Shadow::default(),
            }
        })
        .into()
}

/// Right-side lyrics panel (FillPortion(2)) for the Now Playing view.
///
/// | `lyrics`         | Displayed content                       |
/// |------------------|-----------------------------------------|
/// | `None`           | "Loading…" (not fetched yet)            |
/// | `Some(None)`     | "No lyrics available"                   |
/// | `Some(Some(l))`  | synced (highlighted) or plain lyric text|
///
/// When synced lyrics are available, the line matching `elapsed` (seconds into
/// the track) is highlighted in the accent colour; the rest are muted.
fn lyrics_column<'a>(
    lyrics: Option<Option<&'a crate::lyrics::Lyrics>>,
    elapsed: f64,
    scroll_id: iced::widget::scrollable::Id,
) -> Element<'a, Message> {
    let inner: Element<'a, Message> = match lyrics {
        None => centered_note("Loading lyrics…"),
        Some(None) => centered_note("No lyrics available"),
        Some(Some(l)) if l.instrumental => centered_note("♪ Instrumental"),

        Some(Some(l)) => {
            // Prefer synced (for highlighting); fall back to plain text.
            if let Some(ref synced) = l.synced {
                // Active line = the last one whose timestamp has passed,
                // nudged by LYRIC_SYNC_OFFSET so the highlight doesn't run
                // ahead of the vocals.
                let t = elapsed - LYRIC_SYNC_OFFSET;
                let active = synced.iter().rposition(|line| line.secs <= t);

                let mut col = column![].spacing(8).width(Length::Fill);
                for (i, line) in synced.iter().enumerate() {
                    // Blank lines act as spacers between verses.
                    if line.text.is_empty() {
                        col = col.push(Space::with_height(6));
                        continue;
                    }
                    let is_active = Some(i) == active;
                    let (color, size) = if is_active {
                        (AppColors::ACCENT, 17)
                    } else {
                        (AppColors::TEXT_MUTED, 15)
                    };
                    col = col.push(text(&line.text).size(size).color(color));
                }
                scrollable(col)
                    .id(scroll_id)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            } else if let Some(ref plain) = l.plain {
                scrollable(text(plain).size(15).color(AppColors::TEXT_SECONDARY))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            } else {
                centered_note("No lyrics available")
            }
        }
    };

    // Wrap in a subtle panel so the lyrics read as a distinct pane, not an
    // empty black void next to the song info.
    container(inner)
        .width(Length::FillPortion(2))
        .height(Length::Fill)
        .padding(16)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppColors::BG_SECONDARY.into()),
            border: iced::Border {
                radius: 6.0.into(),
                color: AppColors::BORDER,
                width: 1.0,
            },
            ..Default::default()
        })
        .into()
}

/// A short note centered in the lyrics pane (loading / not found / instrumental).
fn centered_note<'a>(msg: &'a str) -> Element<'a, Message> {
    container(text(msg).size(14).color(AppColors::TEXT_MUTED))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Technical audio details: format · sample rate · bit depth · channels · bitrate
fn format_tech_line(song: &Song, status: &Status) -> String {
    let mut parts = Vec::new();

    let fmt = song.display_format();
    if !fmt.is_empty() {
        parts.push(fmt);
    }

    if let Some(audio) = &status.audio {
        parts.push(format!("{} Hz", audio.sample_rate));
        parts.push(format!("{} bit", audio.bits));
        parts.push(format!("{} ch", audio.channels));
    }

    if let Some(br) = status.bitrate {
        parts.push(format!("{br} kbps"));
    }

    parts.join("  ·  ")
}

/// Genre and year metadata line.
fn format_meta_line(song: &Song) -> String {
    let mut parts = Vec::new();
    if let Some(genre) = &song.genre {
        parts.push(genre.clone());
    }
    if let Some(date) = &song.date {
        // Show just the year (first 4 chars) if it looks like a full date
        let year = if date.len() > 4 { &date[..4] } else { date.as_str() };
        parts.push(year.to_string());
    }
    parts.join("  ·  ")
}

/// Truncate a string at `max_chars`, appending "…" if it was cut.
fn truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{collected}…")
    } else {
        collected
    }
}
