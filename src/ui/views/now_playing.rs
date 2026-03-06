use crate::mpd::types::*;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{column, container, image, row, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    current_song: &'a Option<Song>,
    status: &'a Status,
    art_data: Option<&'a iced::widget::image::Handle>,
) -> Element<'a, Message> {
    let content = match current_song {
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

            let info = column![
                text(song.display_title())
                    .size(28)
                    .color(AppColors::TEXT_PRIMARY),
                Space::with_height(8),
                text(song.display_artist())
                    .size(20)
                    .color(AppColors::ACCENT),
                Space::with_height(4),
                text(song.display_album())
                    .size(18)
                    .color(AppColors::TEXT_SECONDARY),
                Space::with_height(12),
                text(format_status_line(song, status))
                    .size(14)
                    .color(AppColors::TEXT_MUTED),
            ]
            .spacing(2);

            column![
                Space::with_height(40),
                row![art_widget, Space::with_width(30), info]
                    .align_y(Alignment::Center),
            ]
            .align_x(Alignment::Center)
            .width(Length::Fill)
        }
        None => column![
            Space::with_height(100),
            text("Nothing playing")
                .size(24)
                .color(AppColors::TEXT_MUTED),
            Space::with_height(8),
            text("Add songs to the queue and press play")
                .size(16)
                .color(AppColors::TEXT_MUTED),
        ]
        .align_x(Alignment::Center)
        .width(Length::Fill),
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(30)
        .into()
}

fn format_status_line(song: &Song, status: &Status) -> String {
    let mut parts = Vec::new();
    if let Some(genre) = &song.genre {
        parts.push(genre.clone());
    }
    if let Some(date) = &song.date {
        parts.push(date.clone());
    }
    if let Some(audio) = &status.audio {
        parts.push(format!(
            "{}Hz / {}bit / {}ch",
            audio.sample_rate, audio.bits, audio.channels
        ));
    }
    if let Some(br) = status.bitrate {
        parts.push(format!("{br}kbps"));
    }
    parts.join("  |  ")
}
