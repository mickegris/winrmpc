use crate::mpd::types::Partition;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    partitions: &'a [Partition],
    current_partition: &'a str,
    new_partition_name: &'a str,
) -> Element<'a, Message> {
    let mut partition_list = column![].spacing(4);
    for p in partitions {
        let is_current = p.name == current_partition;
        let name_color = if is_current {
            AppColors::ACCENT
        } else {
            AppColors::TEXT_PRIMARY
        };

        let indicator = if is_current { "> " } else { "  " };

        let mut r = row![
            text(indicator.to_string())
                .size(14)
                .width(20)
                .color(AppColors::ACCENT),
            text(&p.name)
                .size(16)
                .color(name_color)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if !is_current {
            r = r.push(
                button(text("Switch").size(13))
                    .on_press(Message::SwitchPartition(p.name.clone()))
                    .padding([4, 10]),
            );
            r = r.push(
                button(text("Delete").size(13).color(AppColors::ERROR))
                    .on_press(Message::DeletePartition(p.name.clone()))
                    .padding([4, 10]),
            );
        }

        partition_list = partition_list.push(r);
    }

    let new_partition_row = row![
        text_input("New partition name", new_partition_name)
            .on_input(Message::PartitionNameInput)
            .size(14)
            .padding(8)
            .width(Length::Fill),
        button(text("Create").size(13))
            .on_press(Message::NewPartition(new_partition_name.to_string()))
            .padding([8, 16]),
    ]
    .spacing(8);

    container(
        column![
            text("Partitions").size(24).color(AppColors::TEXT_PRIMARY),
            Space::with_height(8),
            text("Manage MPD partitions. Each partition has its own queue, player, and outputs.")
                .size(13)
                .color(AppColors::TEXT_SECONDARY),
            Space::with_height(16),
            partition_list,
            Space::with_height(20),
            text("Create New Partition")
                .size(16)
                .color(AppColors::TEXT_PRIMARY),
            Space::with_height(8),
            new_partition_row,
        ]
        .spacing(4)
        .padding(20),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
