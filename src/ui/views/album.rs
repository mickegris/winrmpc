use crate::mpd::types::Song;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, container, image, row, text, Column, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    album_name: &'a str,
    songs: &'a [Song],
    art_handle: Option<&'a iced::widget::image::Handle>,
    bio: Option<&'a str>,
    show_bio: bool,
) -> Element<'a, Message> {
    let artist = songs
        .first()
        .map(|s| s.display_album_artist())
        .unwrap_or("Unknown Artist");

    let total_duration: u64 = songs
        .iter()
        .filter_map(|s| s.duration())
        .map(|d| d.as_secs())
        .sum();
    let total_mins = total_duration / 60;

    // Fixed header
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
    header = header.push(text(album_name).size(22).color(AppColors::TEXT_PRIMARY));
    header = header.push(Space::with_height(4));
    header = header.push(
        button(text(artist).size(16).color(AppColors::ACCENT))
            .on_press(Message::ArtistSelected(artist.to_string()))
            .padding(0)
            .style(|_theme: &iced::Theme, _status| button::Style {
                background: None,
                text_color: AppColors::ACCENT,
                border: iced::Border::default(),
                ..Default::default()
            }),
    );
    header = header.push(Space::with_height(4));
    header = header.push(
        text(format!("{} tracks  |  {} min", songs.len(), total_mins))
            .size(13)
            .color(AppColors::TEXT_MUTED),
    );
    header = header.push(Space::with_height(8));

    header = header.push(
        row![
            button(text("Play All").size(13))
                .on_press(Message::PlayAlbum(album_name.to_string()))
                .padding([6, 16]),
            Space::with_width(8),
            button(text("Queue All").size(13))
                .on_press(Message::QueueAlbum(album_name.to_string()))
                .padding([6, 16]),
        ]
        .spacing(4),
    );

    // Bio section
    header = header.push(Space::with_height(8));
    match bio {
        Some(bio_text) => {
            let toggle_label = if show_bio { "Hide info" } else { "Show info" };
            header = header.push(
                button(text(toggle_label).size(12).color(AppColors::ACCENT))
                    .on_press(Message::ToggleAlbumBio)
                    .padding([4, 8])
                    .style(|_theme: &iced::Theme, _status| button::Style {
                        background: None,
                        text_color: AppColors::ACCENT,
                        border: iced::Border::default(),
                        ..Default::default()
                    }),
            );
            if show_bio {
                header = header.push(Space::with_height(4));
                header = header.push(
                    container(
                        text(bio_text).size(12).color(AppColors::TEXT_SECONDARY),
                    )
                    .padding(12)
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(AppColors::BG_SECONDARY.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            color: AppColors::BORDER,
                            width: 1.0,
                        },
                        ..Default::default()
                    }),
                );
            }
        }
        None => {}
    }

    // Scrollable track list
    let mut track_list = Column::new().spacing(0);

    if songs.is_empty() {
        track_list = track_list.push(
            container(
                text("Loading tracks...").size(14).color(AppColors::TEXT_MUTED),
            )
            .padding([10, 20]),
        );
    }

    for (i, song) in songs.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };
        let track_num = song.track.as_deref().unwrap_or("-");

        track_list = track_list.push(
            button(
                row![
                    text(track_num.to_string())
                        .size(13)
                        .width(30)
                        .color(AppColors::TEXT_MUTED),
                    text(song.display_title())
                        .size(13)
                        .width(Length::Fill)
                        .color(AppColors::TEXT_PRIMARY),
                    text(song.format_duration())
                        .size(12)
                        .color(AppColors::TEXT_MUTED),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::QueueAddUri(song.file.clone()))
            .padding([6, 12])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme, _status| button::Style {
                background: Some(bg.into()),
                text_color: AppColors::TEXT_PRIMARY,
                border: iced::Border::default(),
                ..Default::default()
            }),
        );
    }

    iced::widget::column![
        header,
        iced::widget::scrollable(
            container(track_list).padding([0, 20])
        )
        .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
