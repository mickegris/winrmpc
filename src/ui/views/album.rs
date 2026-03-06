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

    let mut col = Column::new().spacing(0).padding(20);

    col = col.push(
        button(text("<- Back").size(14).color(AppColors::ACCENT))
            .on_press(Message::GoBack)
            .padding([4, 8]),
    );
    col = col.push(Space::with_height(12));

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

    col = col.push(art);
    col = col.push(Space::with_height(12));
    col = col.push(text(album_name).size(22).color(AppColors::TEXT_PRIMARY));
    col = col.push(Space::with_height(4));
    col = col.push(
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
    col = col.push(Space::with_height(4));
    col = col.push(
        text(format!("{} tracks  |  {} min", songs.len(), total_mins))
            .size(13)
            .color(AppColors::TEXT_MUTED),
    );
    col = col.push(Space::with_height(8));
    col = col.push(
        button(text("Play All").size(13))
            .on_press(Message::QueueAddUri(
                songs
                    .first()
                    .map(|s| {
                        if let Some(pos) = s.file.rfind('/') {
                            s.file[..pos].to_string()
                        } else {
                            s.file.clone()
                        }
                    })
                    .unwrap_or_default(),
            ))
            .padding([6, 16]),
    );

    // Wikipedia bio - expandable
    col = col.push(Space::with_height(12));
    match bio {
        Some(bio_text) => {
            let toggle_label = if show_bio { "Hide info" } else { "Show info" };
            col = col.push(
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
                col = col.push(Space::with_height(8));
                col = col.push(
                    container(
                        text(bio_text)
                            .size(12)
                            .color(AppColors::TEXT_SECONDARY),
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

    col = col.push(Space::with_height(16));

    if songs.is_empty() {
        col = col.push(
            text("Loading tracks...").size(14).color(AppColors::TEXT_MUTED),
        );
    }

    for (i, song) in songs.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };
        let track_num = song.track.as_deref().unwrap_or("-");

        col = col.push(
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

    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
