use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    genre_name: &'a str,
    albums: &'a [String],
) -> Element<'a, Message> {
    let mut list = column![].spacing(0);
    for (i, album) in albums.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        list = list.push(
            button(
                text(album.as_str())
                    .size(14)
                    .color(AppColors::TEXT_PRIMARY),
            )
            .on_press(Message::AlbumSelected(album.clone()))
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
                text(genre_name).size(24).color(AppColors::TEXT_PRIMARY),
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
