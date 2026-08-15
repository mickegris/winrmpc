//! Shared "Add to Playlist" picker, reachable from Now Playing, albums,
//! the queue, and search — mirrors mikMPD's `AddToPlaylistSheet`.

use crate::mpd::types::PlaylistInfo;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    playlists: &'a [PlaylistInfo],
    new_playlist_name: &'a str,
    song_count: usize,
) -> Element<'a, Message> {
    let title = if song_count == 1 {
        "Add to Playlist".to_string()
    } else {
        format!("Add {song_count} Songs")
    };

    let header = row![
        button(text("Cancel").size(14).color(AppColors::accent()))
            .on_press(Message::CloseAddToPlaylist)
            .padding([4, 8]),
        Space::with_width(12),
        text(title).size(20).color(AppColors::text_primary()),
    ]
    .align_y(Alignment::Center)
    .padding([12, 12]);

    let new_form = row![
        text_input("New playlist name", new_playlist_name)
            .on_input(Message::NewPlaylistNameChanged)
            .on_submit(Message::AddToNewPlaylist)
            .padding(8)
            .width(Length::Fill),
        Space::with_width(8),
        button(text("Add").size(13))
            .on_press(Message::AddToNewPlaylist)
            .padding([8, 16]),
    ]
    .align_y(Alignment::Center);

    let mut list = column![].spacing(4);
    if playlists.is_empty() {
        list = list.push(
            text("No playlists yet").size(13).color(AppColors::text_muted()),
        );
    } else {
        for pl in playlists {
            list = list.push(
                button(text(pl.name.clone()).size(14).color(AppColors::text_primary()))
                    .on_press(Message::AddToPlaylistConfirm(pl.name.clone()))
                    .padding([8, 12])
                    .width(Length::Fill)
                    .style(|_t: &iced::Theme, status: button::Status| {
                        let bg = match status {
                            button::Status::Hovered | button::Status::Pressed => {
                                Some(AppColors::bg_hover().into())
                            }
                            _ => Some(AppColors::bg_secondary().into()),
                        };
                        button::Style {
                            background: bg,
                            text_color: AppColors::text_primary(),
                            border: iced::Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            shadow: iced::Shadow::default(),
                        }
                    }),
            );
        }
    }

    container(
        column![
            header,
            container(new_form).padding(iced::Padding {
                top: 0.0,
                right: 12.0,
                bottom: 16.0,
                left: 12.0,
            }),
            container(
                text("Existing playlists")
                    .size(12)
                    .color(AppColors::text_muted())
                    .width(Length::Fill)
            )
            .padding([0, 12]),
            Space::with_height(8),
            container(scrollable(list).height(Length::Fill)).padding([0, 12]),
        ]
        .spacing(4)
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 12.0,
            left: 0.0,
        }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
