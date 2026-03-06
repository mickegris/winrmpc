use crate::ui::message::{Message, View};
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>() -> Element<'a, Message> {
    let artists_card = make_card("Artists", "Browse by artist", View::Artists);
    let albums_card = make_card("Albums", "Browse by album", View::Albums);
    let genres_card = make_card("Genres", "Browse by genre", View::Genres);
    let folders_card = make_card("Folders", "Browse file tree", View::Browser);
    let search_card = make_card("Search", "Search library", View::Search);

    container(
        column![
            text("Library").size(28).color(AppColors::TEXT_PRIMARY),
            Space::with_height(30),
            row![
                artists_card,
                Space::with_width(16),
                albums_card,
                Space::with_width(16),
                genres_card,
            ]
            .align_y(Alignment::Center),
            Space::with_height(16),
            row![
                folders_card,
                Space::with_width(16),
                search_card,
            ]
            .align_y(Alignment::Center),
        ]
        .padding(30),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn make_card<'a>(label: &'a str, count: &'a str, target: View) -> Element<'a, Message> {
    button(
        container(
            column![
                text(label.to_string()).size(22).color(AppColors::TEXT_PRIMARY),
                Space::with_height(4),
                text(count.to_string()).size(14).color(AppColors::TEXT_MUTED),
            ]
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(20),
    )
    .on_press(Message::NavigateTo(target))
    .width(200)
    .height(120)
    .into()
}
