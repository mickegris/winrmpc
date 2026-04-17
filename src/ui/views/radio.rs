//! Radio stations view: lists built-in and custom stations, allows adding/removing custom ones.

use crate::config::settings::RadioStation;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Element, Length};

pub fn view<'a>(
    stations: &'a [RadioStation],
    add_name: &'a str,
    add_url: &'a str,
) -> Element<'a, Message> {
    let title = text("Radio Stations")
        .size(24)
        .color(AppColors::TEXT_PRIMARY);

    let subtitle = text("Click a station to clear the queue and start streaming.")
        .size(12)
        .color(AppColors::TEXT_SECONDARY);

    // Station list
    let mut station_rows = column![].spacing(4);

    for station in stations {
        let name_label = text(&station.name)
            .size(14)
            .color(AppColors::TEXT_PRIMARY)
            .width(Length::FillPortion(2));

        let url_label = text(&station.url)
            .size(12)
            .color(AppColors::TEXT_SECONDARY)
            .width(Length::FillPortion(3));

        let play_btn = button(text("Play").size(12))
            .on_press(Message::RadioPlay(station.url.clone()))
            .padding([4, 12]);

        let add_to_queue_btn = button(text("+Queue").size(12))
            .on_press(Message::QueueAddUri(station.url.clone()))
            .padding([4, 8]);

        let mut row_items = row![name_label, url_label, play_btn, add_to_queue_btn]
            .spacing(8)
            .align_y(iced::Alignment::Center);

        // Only show remove button for custom (non-builtin) stations
        if !station.is_builtin {
            let remove_btn = button(text("Remove").size(12))
                .on_press(Message::RadioRemoveStation(station.url.clone()))
                .padding([4, 8]);
            row_items = row_items.push(remove_btn);
        }

        let row_container = container(row_items)
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

        station_rows = station_rows.push(row_container);
    }

    // Add custom station form
    let add_section_title = text("Add Custom Station")
        .size(16)
        .color(AppColors::TEXT_PRIMARY);

    let name_input = text_input("Station name", add_name)
        .on_input(Message::RadioAddCustomName)
        .padding(8);

    let url_input = text_input("Stream URL (e.g. https://...)", add_url)
        .on_input(Message::RadioAddCustomUrl)
        .on_submit(Message::RadioAddCustomSubmit)
        .padding(8);

    let add_btn = button(text("Add Station").size(14))
        .on_press(Message::RadioAddCustomSubmit)
        .padding([8, 20]);

    let add_form = column![
        add_section_title,
        Space::with_height(8),
        name_input,
        Space::with_height(4),
        url_input,
        Space::with_height(8),
        add_btn,
    ]
    .spacing(2);

    let content = column![
        title,
        Space::with_height(4),
        subtitle,
        Space::with_height(16),
        scrollable(station_rows).height(Length::Fill),
        Space::with_height(20),
        add_form,
    ]
    .spacing(4)
    .padding(20)
    .width(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
