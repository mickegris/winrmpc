//! Output management view.
use crate::mpd::types::{Output, Partition};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::page::page;
use crate::ui::widgets::icon;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(outputs: &'a [Output], partitions: &'a [Partition]) -> Element<'a, Message> {
    let mut output_list = column![].spacing(8);

    for output in outputs {
        let status_color = if output.enabled {
            AppColors::success()
        } else {
            AppColors::text_muted()
        };
        let status_text = if output.enabled { "Enabled" } else { "Disabled" };

        // Build one "Move to X" button per partition
        let mut move_buttons = row![].spacing(4);
        for partition in partitions {
            move_buttons = move_buttons.push(
                button(
                    row![
                        icon::icon_sized(icon::ARROW_FORWARD, 12),
                        text(partition.name.clone()).size(11),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                    .on_press(Message::MoveOutput {
                        output_id: output.id,
                        output_name: output.name.clone(),
                        target_partition: partition.name.clone(),
                        was_enabled: output.enabled,
                    })
                    .padding([4, 10]),
            );
        }

        let move_row: Element<Message> = if partitions.is_empty() {
            text("No partitions").size(11).color(AppColors::text_muted()).into()
        } else {
            move_buttons.into()
        };

        output_list = output_list.push(
            container(
                column![
                    row![
                        column![
                            text(&output.name)
                                .size(16)
                                .color(AppColors::text_primary()),
                            text(format!("Plugin: {} | ID: {}", output.plugin, output.id))
                                .size(12)
                                .color(AppColors::text_muted()),
                        ]
                        .width(Length::Fill),
                        text(status_text).size(14).color(status_color),
                        Space::with_width(12),
                        button(
                            text(if output.enabled { "Disable" } else { "Enable" }).size(13),
                        )
                        .on_press(Message::ToggleOutput(output.id))
                        .padding([6, 14]),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    Space::with_height(6),
                    row![
                        text("Move to:").size(11).color(AppColors::text_secondary()),
                        Space::with_width(8),
                        move_row,
                    ]
                    .align_y(Alignment::Center),
                ]
                .spacing(2),
            )
            .padding(12)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(AppColors::bg_secondary().into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }

    page(
        column![
            text("Outputs").size(24).color(AppColors::text_primary()),
            Space::with_height(8),
            text("Enable/disable outputs or move them to a partition.")
                .size(13)
                .color(AppColors::text_secondary()),
            Space::with_height(16),
            output_list,
        ]
        .spacing(4)
        .padding(20),
    )
}
