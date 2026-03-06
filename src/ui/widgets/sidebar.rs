use crate::ui::message::{Message, View};
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(current_view: &View, connected: bool, mpd_addr: &str) -> Element<'a, Message> {
    let status_text = if connected {
        text(format!("Connected\n{mpd_addr}"))
            .size(9)
            .color(AppColors::SUCCESS)
    } else {
        text("Not connected")
            .size(9)
            .color(AppColors::ERROR)
    };

    container(
        column![
            Space::with_height(12),
            container(status_text).center_x(Length::Fill),
            Space::with_height(16),
            nav_button("Now Playing", View::NowPlaying, current_view),
            nav_button("Queue", View::Queue, current_view),
            nav_button("Library", View::Library, current_view),
            nav_button("Browse", View::Browser, current_view),
            nav_button("Search", View::Search, current_view),
            Space::with_height(Length::Fill),
            nav_button("Outputs", View::Outputs, current_view),
            nav_button("Partitions", View::Partitions, current_view),
            nav_button("Settings", View::Settings, current_view),
            Space::with_height(8),
        ]
        .spacing(1)
        .align_x(Alignment::Center)
        .width(Length::Fill),
    )
    .width(90)
    .height(Length::Fill)
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(AppColors::BG_SECONDARY.into()),
        ..Default::default()
    })
    .into()
}

fn nav_button<'a>(
    label: &str,
    target: View,
    current_view: &View,
) -> Element<'a, Message> {
    let is_active = current_view == &target;

    let bg = if is_active {
        AppColors::BG_TERTIARY
    } else {
        AppColors::BG_SECONDARY
    };
    let fg = if is_active {
        AppColors::ACCENT
    } else {
        AppColors::TEXT_SECONDARY
    };

    button(
        container(
            text(label.to_string())
                .size(11)
                .color(fg)
                .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .padding([10, 4]),
    )
    .on_press(Message::NavigateTo(target))
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme, _status| button::Style {
        background: Some(bg.into()),
        text_color: fg,
        border: iced::Border::default(),
        ..Default::default()
    })
    .into()
}
