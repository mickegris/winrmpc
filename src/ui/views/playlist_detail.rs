//! Stored-playlist detail: art + track count/duration, Play/Add, per-track actions.
//!
//! # Missing files
//!
//! Stored playlists outlive the files in them. MPD answers an entry whose file
//! is gone with a bare `file:` line and silently skips it on `load`
//! ([`Song::is_missing_from_library`]). Such an entry used to render as an
//! ordinary row titled with its filename and a `--:--` duration, with no hint
//! that it would never play.
//!
//! It is now marked: a warning glyph, the filename, "Missing file" and the
//! folder the file was in — which is what someone restoring it needs. The row
//! is **shown rather than hidden**, so the list still matches what was saved and
//! the dead entry can be found and removed. Ported from mikMPD, with the
//! desktop equivalents of its touch affordances: its tap-to-explain dialog is a
//! tooltip here, and the Remove it offers is the row's own Remove button, which
//! every row already carries.

use crate::mpd::types::Song;
use crate::ui::widgets::link;
use crate::ui::widgets::page::page;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::ui::widgets::link::{icon_btn_danger, icon_btn_tip_maybe, with_tip};
use crate::ui::widgets::song_row;
use iced::widget::{button, column, container, image, row, text, Column, Space};
use iced::{Alignment, Element, Length};

/// Hover text on a missing entry — mikMPD shows the same explanation when one
/// is tapped.
const MISSING_TIP: &str =
    "No longer in the music library — moved, renamed or deleted — so it can't be played or queued.";

pub fn view<'a>(
    playlist_name: &'a str,
    songs: &'a [Song],
    art_handle: Option<&'a iced::widget::image::Handle>,
    current_file: Option<&'a str>,
) -> Element<'a, Message> {
    let total_duration: u64 = songs
        .iter()
        .filter_map(|s| s.duration())
        .map(|d| d.as_secs())
        .sum();
    let missing = songs.iter().filter(|s| s.is_missing_from_library()).count();

    let mut header = Column::new().spacing(2).padding(20);

    header = header.push(
        link::back_button(),
    );
    header = header.push(Space::with_height(12));

    let art: Element<'a, Message> = match art_handle {
        Some(handle) => image(handle.clone()).width(200).height(200).into(),
        None => container(text("").size(1))
            .width(200)
            .height(200)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(AppColors::bg_primary().into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
    };

    header = header.push(art);
    header = header.push(Space::with_height(12));
    header = header.push(text(playlist_name).size(22).color(AppColors::text_primary()));
    header = header.push(Space::with_height(4));
    header = header.push(
        text(track_summary(songs.len(), missing, total_duration / 60))
            .size(13)
            .color(AppColors::text_muted()),
    );
    if let Some(note) = missing_note(missing) {
        header = header.push(Space::with_height(4));
        header = header.push(
            row![
                icon::icon_sized(icon::WARNING, 13).color(AppColors::warning()),
                text(note).size(12).color(AppColors::warning()),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    header = header.push(Space::with_height(8));

    header = header.push(
        row![
            button(text("Play").size(13))
                .on_press(Message::PlaylistPlay(playlist_name.to_string()))
                .padding([6, 16]),
            Space::with_width(8),
            button(text("Add to Queue").size(13))
                .on_press(Message::PlaylistAppend(playlist_name.to_string()))
                .padding([6, 16]),
        ]
        .spacing(4),
    );

    let mut track_list = Column::new().spacing(0);

    if songs.is_empty() {
        track_list = track_list.push(
            container(text("Empty playlist").size(14).color(AppColors::text_muted()))
                .padding([10, 20]),
        );
    }

    for (i, song) in songs.iter().enumerate() {
        let is_missing = song.is_missing_from_library();
        // Match on the URI, **never** on `pos`: `pos` here is the *playlist*
        // index, which has no relationship to `status.song_pos`. Comparing the
        // two would highlight an arbitrary unrelated row — see the test in
        // `widgets::song_row`.
        let is_current = !is_missing && song_row::is_current_uri(&song.file, current_file);
        let bg = song_row::row_bg(i, is_current);
        // `list_playlist` guarantees `pos` is always set (assigned from the
        // record index if the server omits it), so this fallback never fires
        // in practice — it's here only to make the row build if it did.
        let pos = song.pos.unwrap_or(i as u32);
        // MPD would reject a missing file, so the actions that hand it to the
        // queue or another playlist are disabled — never omitted, which would
        // shift this row's columns against its neighbours. Moving and removing
        // still apply: removal is the whole point of marking it.
        let playable = !is_missing;

        // This row used to split its actions across *both* sides of the title —
        // play/add/next leading, move/playlist/remove trailing. All seven now
        // sit together, in the same slot every other track list uses.
        let actions = row![
            icon_btn_tip_maybe(
                icon::ADD_QUEUE,
                "Add to end of queue",
                playable.then(|| Message::QueueAddOnly(song.file.clone()))
            ),
            icon_btn_tip_maybe(
                icon::PLAY_NEXT,
                "Play next",
                playable.then(|| Message::QueueAddNext(song.file.clone()))
            ),
            // Disabled at the ends like the queue's: moving the first entry up
            // is already a no-op in the handler, and moving the last one down
            // sent MPD a range it could only reject.
            icon_btn_tip_maybe(
                icon::MOVE_UP,
                "Move up in playlist",
                (i > 0).then(|| Message::PlaylistMoveSongUp(playlist_name.to_string(), pos))
            ),
            icon_btn_tip_maybe(
                icon::MOVE_DOWN,
                "Move down in playlist",
                (i + 1 < songs.len())
                    .then(|| Message::PlaylistMoveSongDown(playlist_name.to_string(), pos))
            ),
            icon_btn_tip_maybe(
                icon::ADD_PLAYLIST,
                "Add to playlist…",
                playable.then(|| Message::OpenAddToPlaylist(vec![song.file.clone()]))
            ),
            icon_btn_danger(
                icon::REMOVE,
                "Remove from playlist",
                Message::PlaylistRemoveSong(playlist_name.to_string(), pos)
            ),
        ]
        .spacing(song_row::ACTION_SPACING)
        .width(song_row::action_group_width(6));

        let title: Element<'a, Message> = if is_missing {
            with_tip(
                column![
                    row![
                        icon::icon_sized(icon::WARNING, 13).color(AppColors::warning()),
                        text(song.display_title()).size(13).color(AppColors::text_muted()),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    text(missing_caption(&song.file))
                        .size(11)
                        .color(AppColors::warning()),
                ]
                .spacing(2)
                .width(Length::Fill)
                .into(),
                MISSING_TIP,
            )
        } else {
            text(song.display_title())
                .size(13)
                .width(Length::Fill)
                .color(song_row::title_color(is_current))
                .into()
        };

        track_list = track_list.push(
            container(
                row![
                    song_row::playing_marker(is_current),
                    icon_btn_tip_maybe(
                        icon::PLAY,
                        "Play now",
                        playable.then(|| Message::PlaylistPlayAt(playlist_name.to_string(), pos))
                    ),
                    song_row::number((pos + 1).to_string(), 13),
                    title,
                    // Blank rather than `--:--`: there is no file to have a length.
                    song_row::duration(if is_missing { String::new() } else { song.format_duration() }, 12),
                    actions,
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([6, 12])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                ..Default::default()
            }),
        );
    }

    page(iced::widget::column![
        header,
        container(track_list).padding([0, 20]),
    ])
}

/// The header's count line. **Counts what Play will actually play**, and says
/// separately how many are gone — "417 tracks" for a playlist that loads as
/// 412 would be quietly wrong.
fn track_summary(total: usize, missing: usize, minutes: u64) -> String {
    if missing == 0 {
        format!("{total} tracks  |  {minutes} min")
    } else {
        format!("{} tracks  |  {missing} missing  |  {minutes} min", total - missing)
    }
}

/// The header's explanation, shown only when something is missing.
fn missing_note(missing: usize) -> Option<String> {
    let lead = match missing {
        0 => return None,
        1 => "One file in this playlist is".to_string(),
        n => format!("{n} files in this playlist are"),
    };
    Some(format!(
        "{lead} no longer in the library, so Play skips {}. Marked below with the folder {} in.",
        if missing == 1 { "it" } else { "them" },
        if missing == 1 { "it was" } else { "each was" },
    ))
}

/// The second line of a missing row: "Missing file", plus the folder the file
/// was in when there is one.
fn missing_caption(file: &str) -> String {
    match file.rsplit_once('/') {
        Some((folder, _)) if !folder.is_empty() => format!("Missing file · {folder}"),
        _ => "Missing file".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playlist_with_nothing_missing_reads_as_before() {
        assert_eq!(track_summary(12, 0, 48), "12 tracks  |  48 min");
        assert_eq!(missing_note(0), None);
    }

    /// The count is what `load` will queue, not the number of entries.
    #[test]
    fn the_track_count_excludes_missing_entries() {
        assert_eq!(track_summary(417, 5, 1500), "412 tracks  |  5 missing  |  1500 min");
    }

    #[test]
    fn the_note_agrees_in_number() {
        assert!(missing_note(1).unwrap().starts_with("One file in this playlist is "));
        assert!(missing_note(5).unwrap().starts_with("5 files in this playlist are "));
    }

    #[test]
    fn the_caption_names_the_folder_the_file_was_in() {
        assert_eq!(
            missing_caption("Peter Gabriel/Us/04 - Steam.flac"),
            "Missing file · Peter Gabriel/Us"
        );
        assert_eq!(missing_caption("loose.flac"), "Missing file");
        assert_eq!(missing_caption("/loose.flac"), "Missing file");
    }
}
