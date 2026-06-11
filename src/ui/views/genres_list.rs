use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(genres: &'a [String]) -> Element<'a, Message> {
    let mut list = column![].spacing(0);
    for (i, genre) in genres.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        list = list.push(
            button(
                text(genre.as_str())
                    .size(14)
                    .color(AppColors::TEXT_PRIMARY),
            )
            .on_press(Message::GenreSelected(genre.clone()))
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
                text("Genres").size(24).color(AppColors::TEXT_PRIMARY),
                Space::with_width(12),
                text(format!("{} genres", genres.len()))
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
