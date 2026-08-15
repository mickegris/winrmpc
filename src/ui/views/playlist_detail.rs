//! Stored-playlist detail: art + track count/duration, Play/Add, per-track actions.

use crate::mpd::types::Song;
use crate::ui::widgets::link;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::ui::widgets::link::{icon_btn_danger, icon_btn_tip, icon_btn_tip_maybe};
use crate::ui::widgets::song_row;
use iced::widget::{button, container, image, row, text, Column, Space};
use iced::{Alignment, Element, Length};

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
    let total_mins = total_duration / 60;

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
                background: Some(AppColors::BG_PRIMARY.into()),
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
    header = header.push(text(playlist_name).size(22).color(AppColors::TEXT_PRIMARY));
    header = header.push(Space::with_height(4));
    header = header.push(
        text(format!("{} tracks  |  {} min", songs.len(), total_mins))
            .size(13)
            .color(AppColors::TEXT_MUTED),
    );
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
            container(text("Empty playlist").size(14).color(AppColors::TEXT_MUTED))
                .padding([10, 20]),
        );
    }

    for (i, song) in songs.iter().enumerate() {
        // Match on the URI, **never** on `pos`: `pos` here is the *playlist*
        // index, which has no relationship to `status.song_pos`. Comparing the
        // two would highlight an arbitrary unrelated row — see the test in
        // `widgets::song_row`.
        let is_current = song_row::is_current_uri(&song.file, current_file);
        let bg = song_row::row_bg(i, is_current);
        // `list_playlist` guarantees `pos` is always set (assigned from the
        // record index if the server omits it), so this fallback never fires
        // in practice — it's here only to make the row build if it did.
        let pos = song.pos.unwrap_or(i as u32);

        // This row used to split its actions across *both* sides of the title —
        // play/add/next leading, move/playlist/remove trailing. All seven now
        // sit together, in the same slot every other track list uses.
        let actions = row![
            icon_btn_tip(
                icon::ADD_QUEUE,
                "Add to end of queue",
                Message::QueueAddOnly(song.file.clone())
            ),
            icon_btn_tip(
                icon::PLAY_NEXT,
                "Play next",
                Message::QueueAddNext(song.file.clone())
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
            icon_btn_tip(
                icon::ADD_PLAYLIST,
                "Add to playlist…",
                Message::OpenAddToPlaylist(vec![song.file.clone()])
            ),
            icon_btn_danger(
                icon::REMOVE,
                "Remove from playlist",
                Message::PlaylistRemoveSong(playlist_name.to_string(), pos)
            ),
        ]
        .spacing(song_row::ACTION_SPACING)
        .width(song_row::action_group_width(6));

        track_list = track_list.push(
            container(
                row![
                    song_row::playing_marker(is_current),
                    icon_btn_tip(
                        icon::PLAY,
                        "Play now",
                        Message::PlaylistPlayAt(playlist_name.to_string(), pos)
                    ),
                    song_row::number((pos + 1).to_string(), 13),
                    text(song.display_title())
                        .size(13)
                        .width(Length::Fill)
                        .color(song_row::title_color(is_current)),
                    song_row::duration(song.format_duration(), 12),
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

    iced::widget::column![
        header,
        iced::widget::scrollable(container(track_list).padding([0, 20])).height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
