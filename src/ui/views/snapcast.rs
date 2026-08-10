//! Snapcast multiroom control — per-client volume sliders, mute, and group
//! mute/stream picker. Phase 1+2 of docs/plans/snapcast-control.md
//! (read-only status + basic controls; rename/latency/move/delete deferred).

use crate::snapcast::{SnapClient, SnapGroup};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, pick_list, row, scrollable, slider, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    groups: &'a [SnapGroup],
    streams: &'a [crate::snapcast::SnapStream],
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let header = column![
        text("Snapcast").size(24).color(AppColors::TEXT_PRIMARY),
        Space::with_height(4),
        text("Control Snapcast multiroom clients.")
            .size(13)
            .color(AppColors::TEXT_SECONDARY),
    ]
    .spacing(2);

    let body: Element<'a, Message> = if let Some(err) = error {
        container(
            column![
                text("Snapcast unreachable").size(16).color(AppColors::ERROR),
                Space::with_height(4),
                text(err).size(12).color(AppColors::TEXT_MUTED),
                Space::with_height(8),
                text("Check the Snapcast host/port in this server's settings.")
                    .size(11)
                    .color(AppColors::TEXT_MUTED),
            ],
        )
        .padding(20)
        .into()
    } else if groups.is_empty() {
        container(
            text("No Snapcast groups found.")
                .size(14)
                .color(AppColors::TEXT_MUTED),
        )
        .padding(20)
        .into()
    } else {
        let mut list = column![].spacing(12);
        for group in groups {
            list = list.push(group_section(group, streams));
        }
        scrollable(list).height(Length::Fill).into()
    };

    container(
        column![header, Space::with_height(16), body]
            .spacing(0)
            .padding(20),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn group_section<'a>(
    group: &'a SnapGroup,
    streams: &'a [crate::snapcast::SnapStream],
) -> Element<'a, Message> {
    let mute_label = if group.muted { "Unmute Group" } else { "Mute Group" };
    let group_id = group.id.clone();
    let was_muted = group.muted;

    let mut header = row![
        text(group.display_name()).size(16).color(AppColors::TEXT_PRIMARY),
        Space::with_width(Length::Fill),
    ]
    .align_y(Alignment::Center);

    // Stream picker — only worth showing when there's an actual choice.
    if streams.len() > 1 {
        let names: Vec<String> = streams.iter().map(|s| s.id.clone()).collect();
        let gid = group.id.clone();
        header = header.push(pick_list(names, Some(group.stream_id.clone()), move |stream_id| {
            Message::SnapcastSetGroupStream(gid.clone(), stream_id)
        }));
        header = header.push(Space::with_width(12));
    }

    header = header.push(
        button(text(mute_label).size(12))
            .on_press(Message::SnapcastToggleGroupMute(group_id, was_muted))
            .padding([4, 10]),
    );

    let mut client_rows = column![].spacing(8);
    for client in &group.clients {
        client_rows = client_rows.push(client_row(client));
    }

    container(column![header, Space::with_height(10), client_rows].spacing(0))
        .padding(12)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(AppColors::BG_SECONDARY.into()),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn client_row<'a>(client: &'a SnapClient) -> Element<'a, Message> {
    let dot_color = if client.connected {
        AppColors::SUCCESS
    } else {
        AppColors::TEXT_MUTED
    };
    let name_color = if client.connected {
        AppColors::TEXT_PRIMARY
    } else {
        AppColors::TEXT_MUTED
    };

    let mute_label = if client.muted { "Unmute" } else { "Mute" };
    let latency_label = if client.latency != 0 {
        format!("+{}ms", client.latency)
    } else {
        String::new()
    };

    let slider_id = client.id.clone();
    let mute_id = client.id.clone();
    let was_muted = client.muted;

    row![
        text("●").size(10).color(dot_color),
        text(client.display_name().to_string())
            .size(13)
            .color(name_color)
            .width(140),
        text(latency_label).size(10).color(AppColors::TEXT_MUTED).width(50),
        slider(0u8..=100u8, client.volume, move |v| {
            Message::SnapcastSetVolume(slider_id.clone(), v)
        })
        .width(140)
        .step(1u8),
        text(format!("{}%", client.volume))
            .size(12)
            .color(AppColors::TEXT_MUTED)
            .width(36),
        button(text(mute_label).size(11))
            .on_press(Message::SnapcastToggleClientMute(mute_id, was_muted))
            .padding([3, 10]),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
