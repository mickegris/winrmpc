use crate::mpd::types::*;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    queue: &'a [Song],
    current_pos: Option<u32>,
) -> Element<'a, Message> {
    let toolbar = row![
        text(format!("{} tracks", queue.len()))
            .size(13)
            .color(AppColors::TEXT_MUTED),
        Space::with_width(Length::Fill),
        button(text("Shuffle").size(12))
            .on_press(Message::QueueShuffle)
            .padding([4, 12]),
        button(text("Clear").size(12))
            .on_press(Message::QueueClear)
            .padding([4, 12]),
    ]
    .spacing(8)
    .padding([8, 12]);

    let header = container(
        row![
            text("#").size(11).width(40).color(AppColors::TEXT_MUTED),
            text("Title").size(11).width(Length::FillPortion(3)).color(AppColors::TEXT_MUTED),
            text("Artist").size(11).width(Length::FillPortion(2)).color(AppColors::TEXT_MUTED),
            text("Album").size(11).width(Length::FillPortion(2)).color(AppColors::TEXT_MUTED),
            text("Time").size(11).width(55).color(AppColors::TEXT_MUTED),
        ]
        .spacing(8)
        .padding([4, 12]),
    )
    .style(|_theme: &iced::Theme| container::Style {
        border: iced::Border {
            width: 1.0,
            color: AppColors::BORDER,
            ..Default::default()
        },
        ..Default::default()
    });

    let mut items = column![].spacing(0);

    if queue.is_empty() {
        items = items.push(
            container(
                text("Queue is empty. Add songs from Artists, Albums, Browse, or Search.")
                    .size(14)
                    .color(AppColors::TEXT_MUTED),
            )
            .padding(20),
        );
    }

    for (i, song) in queue.iter().enumerate() {
        let pos = song.pos.unwrap_or(0);
        let is_current = current_pos == Some(pos);

        let bg = if is_current {
            AppColors::BG_TERTIARY
        } else if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        let title_color = if is_current {
            AppColors::ACCENT
        } else {
            AppColors::TEXT_PRIMARY
        };

        items = items.push(
            button(
                row![
                    text(format!("{}", pos + 1))
                        .size(12)
                        .width(40)
                        .color(AppColors::TEXT_MUTED),
                    text(song.display_title())
                        .size(12)
                        .width(Length::FillPortion(3))
                        .color(title_color),
                    text(song.display_artist())
                        .size(11)
                        .width(Length::FillPortion(2))
                        .color(AppColors::TEXT_SECONDARY),
                    text(song.display_album())
                        .size(11)
                        .width(Length::FillPortion(2))
                        .color(AppColors::TEXT_SECONDARY),
                    text(song.format_duration())
                        .size(11)
                        .width(55)
                        .color(AppColors::TEXT_MUTED),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::QueuePlay(pos))
            .padding([5, 12])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme, _status| button::Style {
                background: Some(bg.into()),
                text_color: AppColors::TEXT_PRIMARY,
                border: iced::Border::default(),
                ..Default::default()
            }),
        );
    }

    container(
        column![
            toolbar,
            header,
            scrollable(items).height(Length::Fill),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
