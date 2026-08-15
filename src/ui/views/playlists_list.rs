//! Stored-playlist browser: list of playlists, save-queue-as-playlist form, rename/delete.

use crate::mpd::types::PlaylistInfo;
use crate::ui::widgets::link;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    playlists: &'a [PlaylistInfo],
    new_playlist_name: &'a str,
    renaming: Option<&'a str>,
    rename_input: &'a str,
    queue_empty: bool,
) -> Element<'a, Message> {
    let header = row![
        link::back_button(),
        Space::with_width(12),
        text("Playlists").size(24).color(AppColors::TEXT_PRIMARY),
        Space::with_width(12),
        text(format!("{} playlists", playlists.len()))
            .size(14)
            .color(AppColors::TEXT_MUTED),
    ]
    .align_y(Alignment::Center)
    .padding([12, 12]);

    let mut list = column![].spacing(4);

    if playlists.is_empty() {
        list = list.push(
            container(
                text(
                    "No playlists yet. Save the queue below, or use \"Add to Playlist\" \
                     on a song or album.",
                )
                .size(13)
                .color(AppColors::TEXT_MUTED),
            )
            .padding([10, 12]),
        );
    }

    for pl in playlists {
        let is_renaming = renaming == Some(pl.name.as_str());

        let row_content: Element<'a, Message> = if is_renaming {
            row![
                text_input("Playlist name", rename_input)
                    .on_input(Message::RenamePlaylistInput)
                    .on_submit(Message::ConfirmRenamePlaylist)
                    .padding([4, 8])
                    .size(13)
                    .width(220),
                Space::with_width(6),
                button(text("Save").size(11))
                    .on_press(Message::ConfirmRenamePlaylist)
                    .padding([3, 10]),
                Space::with_width(4),
                button(text("Cancel").size(11))
                    .on_press(Message::CancelRenamePlaylist)
                    .padding([3, 10]),
            ]
            .align_y(Alignment::Center)
            .spacing(4)
            .into()
        } else {
            row![
                button(text(pl.name.clone()).size(14).color(AppColors::TEXT_PRIMARY))
                    .on_press(Message::PlaylistSelected(pl.name.clone()))
                    .padding(0)
                    .width(Length::Fill)
                    .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                        background: None,
                        text_color: AppColors::TEXT_PRIMARY,
                        border: iced::Border::default(),
                        ..Default::default()
                    }),
                button(text("Rename").size(11))
                    .on_press(Message::StartRenamePlaylist(pl.name.clone()))
                    .padding([3, 8])
                    .style(|_t: &iced::Theme, s: button::Status| button::Style {
                        background: None,
                        text_color: match s {
                            button::Status::Hovered | button::Status::Pressed => {
                                AppColors::TEXT_PRIMARY
                            }
                            _ => AppColors::TEXT_MUTED,
                        },
                        border: iced::Border::default(),
                        shadow: iced::Shadow::default(),
                    }),
                Space::with_width(4),
                button(icon::icon_sized(icon::REMOVE, 14))
                    .on_press(Message::PlaylistDelete(pl.name.clone()))
                    .padding([3, 8])
                    .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                        background: None,
                        text_color: AppColors::ERROR,
                        border: iced::Border::default(),
                        ..Default::default()
                    }),
            ]
            .align_y(Alignment::Center)
            .into()
        };

        list = list.push(
            container(row_content)
                .padding([8, 12])
                .width(Length::Fill)
                .style(|_t: &iced::Theme| container::Style {
                    background: Some(AppColors::BG_SECONDARY.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        );
    }

    let save_form = row![
        text_input("Save queue as...", new_playlist_name)
            .on_input(Message::NewPlaylistNameChanged)
            .on_submit(Message::SaveQueueAsPlaylist)
            .padding(8)
            .width(Length::Fill),
        Space::with_width(8),
        button(text("Save Queue").size(13))
            .on_press_maybe((!queue_empty).then_some(Message::SaveQueueAsPlaylist))
            .padding([8, 16]),
    ]
    .align_y(Alignment::Center);

    container(
        column![
            header,
            container(scrollable(list).height(Length::Fill))
                .padding([0, 12])
                .height(Length::Fill),
            Space::with_height(12),
            container(save_form).padding(iced::Padding {
                top: 0.0,
                right: 12.0,
                bottom: 16.0,
                left: 12.0,
            }),
        ]
        .spacing(4),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
