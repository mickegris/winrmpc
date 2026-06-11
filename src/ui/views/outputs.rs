//! Output management view.
use crate::mpd::types::{Output, Partition};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(outputs: &'a [Output], partitions: &'a [Partition]) -> Element<'a, Message> {
    let mut output_list = column![].spacing(8);

    for output in outputs {
        let status_color = if output.enabled {
            AppColors::SUCCESS
        } else {
            AppColors::TEXT_MUTED
        };
        let status_text = if output.enabled { "Enabled" } else { "Disabled" };

        // Build one "Move to X" button per partition
        let mut move_buttons = row![].spacing(4);
        for partition in partitions {
            let btn_label = format!("→ {}", partition.name);
            move_buttons = move_buttons.push(
                button(text(btn_label).size(11))
                    .on_press(Message::MoveOutput {
                        output_name: output.name.clone(),
                        target_partition: partition.name.clone(),
                    })
                    .padding([4, 10]),
            );
        }

        let move_row: Element<Message> = if partitions.is_empty() {
            text("No partitions").size(11).color(AppColors::TEXT_MUTED).into()
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
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    Space::with_height(6),
                    row![
                        text("Move to:").size(11).color(AppColors::TEXT_SECONDARY),
                        Space::with_width(8),
                        move_row,
                    ]
                    .align_y(Alignment::Center),
                ]
                .spacing(2),
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
            text("Enable/disable outputs or move them to a partition.")
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
