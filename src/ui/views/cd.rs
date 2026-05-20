//! CD playback view: play whole disc or probe and play/queue individual tracks.

use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length};

pub fn view<'a>(tracks: &'a [String], probing: bool) -> Element<'a, Message> {
    let title = text("Audio CD")
        .size(24)
        .color(AppColors::TEXT_PRIMARY);

    // Top action row
    let play_whole_btn = button(text("Play Whole CD").size(13))
        .on_press(Message::CdPlayWhole)
        .padding([8, 16]);

    let probe_btn = if probing {
        button(text("Loading…").size(13)).padding([8, 16])
    } else {
        button(text("Load Tracks").size(13))
            .on_press(Message::CdProbe)
            .padding([8, 16])
    };

    let action_row = row![play_whole_btn, probe_btn]
        .spacing(8)
        .align_y(iced::Alignment::Center);

    // Track list
    let track_section: Element<Message> = if probing {
        text("Probing disc…")
            .size(13)
            .color(AppColors::TEXT_SECONDARY)
            .into()
    } else if tracks.is_empty() {
        text("No tracks loaded. Insert a disc and click Load Tracks.")
            .size(13)
            .color(AppColors::TEXT_SECONDARY)
            .into()
    } else {
        let mut track_rows = column![].spacing(4);

        for (i, uri) in tracks.iter().enumerate() {
            let track_num = text(format!("Track {}", i + 1))
                .size(14)
                .color(AppColors::TEXT_PRIMARY)
                .width(Length::Fill);

            let play_btn = button(text("Play").size(12))
                .on_press(Message::CdPlayTrack(uri.clone()))
                .padding([4, 12]);

            let add_btn = button(text("+Queue").size(12))
                .on_press(Message::CdAddTrack(uri.clone()))
                .padding([4, 8]);

            let row_content = row![track_num, play_btn, add_btn]
                .spacing(8)
                .align_y(iced::Alignment::Center);

            let row_container = container(row_content)
                .padding([8, 12])
                .width(Length::Fill)
                .style(|_theme: &iced::Theme| container::Style {
                    background: Some(AppColors::BG_SECONDARY.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });

            track_rows = track_rows.push(row_container);
        }

        scrollable(track_rows).height(Length::Fill).into()
    };

    let content = column![
        title,
        Space::with_height(16),
        action_row,
        Space::with_height(16),
        track_section,
    ]
    .spacing(4)
    .padding(20)
    .width(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
