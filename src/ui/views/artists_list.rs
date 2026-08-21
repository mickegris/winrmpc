use crate::ui::widgets::link;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(artists: &'a [String], sort_desc: bool) -> Element<'a, Message> {
    let mut list = column![].spacing(0);
    for (i, artist) in artists.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::row_even()
        } else {
            AppColors::row_odd()
        };

        list = list.push(
            button(
                text(artist.as_str())
                    .size(14)
                    .color(AppColors::text_primary()),
            )
            .on_press(Message::ArtistSelected(artist.clone()))
            .padding([7, 12])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme, _status| button::Style {
                background: Some(bg.into()),
                text_color: AppColors::text_primary(),
                border: iced::Border::default(),
                ..Default::default()
            }),
        );
    }

    container(
        column![
            row![
                link::back_button(),
                Space::with_width(12),
                text("Artists").size(24).color(AppColors::text_primary()),
                Space::with_width(12),
                text(format!("{} artists", artists.len()))
                    .size(14)
                    .color(AppColors::text_muted()),
                Space::with_width(Length::Fill),
                link::sort_toggle(sort_desc),
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
