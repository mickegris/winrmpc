//! Stored-playlist detail: art + track count/duration, Play/Add, per-track actions.

use crate::mpd::types::Song;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::link::icon_btn;
use iced::widget::{button, container, image, row, text, Column, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    playlist_name: &'a str,
    songs: &'a [Song],
    art_handle: Option<&'a iced::widget::image::Handle>,
) -> Element<'a, Message> {
    let total_duration: u64 = songs
        .iter()
        .filter_map(|s| s.duration())
        .map(|d| d.as_secs())
        .sum();
    let total_mins = total_duration / 60;

    let mut header = Column::new().spacing(2).padding(20);

    header = header.push(
        button(text("<- Back").size(14).color(AppColors::ACCENT))
            .on_press(Message::GoBack)
            .padding([4, 8]),
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
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };
        // `list_playlist` guarantees `pos` is always set (assigned from the
        // record index if the server omits it), so this fallback never fires
        // in practice — it's here only to make the row build if it did.
        let pos = song.pos.unwrap_or(i as u32);

        track_list = track_list.push(
            container(
                row![
                    icon_btn(
                        "▶",
                        Message::PlaylistPlayAt(playlist_name.to_string(), pos)
                    ),
                    icon_btn("+", Message::QueueAddOnly(song.file.clone())),
                    icon_btn("⏭", Message::QueueAddNext(song.file.clone())),
                    text(song.display_title())
                        .size(13)
                        .width(Length::Fill)
                        .color(AppColors::TEXT_PRIMARY),
                    text(song.format_duration())
                        .size(12)
                        .color(AppColors::TEXT_MUTED),
                    icon_btn(
                        "↑",
                        Message::PlaylistMoveSongUp(playlist_name.to_string(), pos)
                    ),
                    icon_btn(
                        "↓",
                        Message::PlaylistMoveSongDown(playlist_name.to_string(), pos)
                    ),
                    button(text("☰").size(13))
                        .on_press(Message::OpenAddToPlaylist(vec![song.file.clone()]))
                        .padding([2, 8])
                        .style(|_t: &iced::Theme, s: button::Status| button::Style {
                            background: None,
                            text_color: match s {
                                button::Status::Hovered | button::Status::Pressed => {
                                    AppColors::ACCENT
                                }
                                _ => AppColors::TEXT_MUTED,
                            },
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                    button(text("×").size(14))
                        .on_press(Message::PlaylistRemoveSong(
                            playlist_name.to_string(),
                            pos
                        ))
                        .padding([2, 8])
                        .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                            background: None,
                            text_color: AppColors::ERROR,
                            border: iced::Border::default(),
                            ..Default::default()
                        }),
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
