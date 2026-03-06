//! Output management view.

use crate::mpd::types::Output;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(outputs: &'a [Output]) -> Element<'a, Message> {
    let mut output_list = column![].spacing(8);
    for output in outputs {
        let status_color = if output.enabled {
            AppColors::SUCCESS
        } else {
            AppColors::TEXT_MUTED
        };
        let status_text = if output.enabled { "Enabled" } else { "Disabled" };

        output_list = output_list.push(
            container(
                row![
                    column![
                        text(&output.name)
                            .size(16)
                            .color(AppColors::TEXT_PRIMARY),
                        text(format!("Plugin: {} | ID: {}", output.plugin, output.id))
                            .size(12)
                            .color(AppColors::TEXT_MUTED),
                    ]
                    .width(Length::Fill),
                    text(status_text).size(14).color(status_color),
                    Space::with_width(12),
                    button(
                        text(if output.enabled { "Disable" } else { "Enable" }).size(13),
                    )
                    .on_press(Message::ToggleOutput(output.id))
                    .padding([6, 14]),
                    Space::with_width(4),
                    button(text("Move Here").size(13))
                        .on_press(Message::MoveOutput(output.name.clone()))
                        .padding([6, 14]),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(12)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(AppColors::BG_SECONDARY.into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }

    container(
        column![
            text("Outputs").size(24).color(AppColors::TEXT_PRIMARY),
            Space::with_height(8),
            text("Manage audio outputs. Move outputs between partitions.")
                .size(13)
                .color(AppColors::TEXT_SECONDARY),
            Space::with_height(16),
            output_list,
        ]
        .spacing(4)
        .padding(20),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
