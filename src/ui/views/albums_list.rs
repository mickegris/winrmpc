use crate::mpd::types::AlbumGroup;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(albums: &'a [AlbumGroup], title: &'a str) -> Element<'a, Message> {
    let mut list = column![].spacing(0);
    for (i, group) in albums.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        let mut label = row![
            text(group.base.as_str()).size(14).color(AppColors::TEXT_PRIMARY),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if !group.artist.is_empty() {
            label = label.push(
                text(group.artist.as_str()).size(12).color(AppColors::TEXT_MUTED),
            );
        }
        if group.variants.len() > 1 {
            label = label.push(
                text(format!("{} discs", group.variants.len()))
                    .size(11)
                    .color(AppColors::ACCENT),
            );
        }

        let artist_opt = if group.artist.is_empty() {
            None
        } else {
            Some(group.artist.clone())
        };

        list = list.push(
            button(label)
                .on_press(Message::AlbumSelected(group.base.clone(), artist_opt))
                .padding([7, 12])
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
            row![
                button(text("<- Back").size(14).color(AppColors::ACCENT))
                    .on_press(Message::GoBack)
                    .padding([4, 8]),
                Space::with_width(12),
                text(title).size(24).color(AppColors::TEXT_PRIMARY),
                Space::with_width(12),
                text(format!("{} albums", albums.len()))
                    .size(14)
                    .color(AppColors::TEXT_MUTED),
            ]
            .align_y(Alignment::Center)
            .padding([12, 12]),
            scrollable(list).height(Length::Fill),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
