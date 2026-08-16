//! Snapcast multiroom control — per-client volume sliders, mute, and group
//! mute/stream picker. Phase 1+2 of docs/plans/snapcast-control.md
//! (read-only status + basic controls; rename/latency/move/delete deferred).

use crate::snapcast::{SnapClient, SnapGroup};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use iced::widget::{button, column, container, pick_list, row, scrollable, slider, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    groups: &'a [SnapGroup],
    streams: &'a [crate::snapcast::SnapStream],
    error: Option<&'a str>,
    show_inactive: bool,
) -> Element<'a, Message> {
    // Disconnected clients accumulate on a Snapcast server: every browser
    // tab or device that ever connected leaves a stale entry behind, so the
    // list is mostly noise by default.
    let inactive_count: usize = groups
        .iter()
        .flat_map(|g| g.clients.iter())
        .filter(|c| !c.connected)
        .count();

    let mut header_row = row![
        text("Snapcast").size(24).color(AppColors::text_primary()),
        Space::with_width(Length::Fill),
    ]
    .align_y(Alignment::Center);
    if inactive_count > 0 {
        let label = if show_inactive {
            format!("Hide {inactive_count} inactive")
        } else {
            format!("Show {inactive_count} inactive")
        };
        header_row = header_row.push(
            button(text(label).size(12))
                .on_press(Message::SnapcastToggleShowInactive)
                .padding([4, 10]),
        );
    }

    let header = column![
        header_row,
        Space::with_height(4),
        text("Control Snapcast multiroom clients.")
            .size(13)
            .color(AppColors::text_secondary()),
    ]
    .spacing(2);

    let body: Element<'a, Message> = if let Some(err) = error {
        container(
            column![
                text("Snapcast unreachable").size(16).color(AppColors::error()),
                Space::with_height(4),
                text(err).size(12).color(AppColors::text_muted()),
                Space::with_height(8),
                text("Check the Snapcast host/port in this server's settings.")
                    .size(11)
                    .color(AppColors::text_muted()),
            ],
        )
        .padding(20)
        .into()
    } else if visible_groups(groups, show_inactive).is_empty() {
        container(
            text("No Snapcast groups found.")
                .size(14)
                .color(AppColors::text_muted()),
        )
        .padding(20)
        .into()
    } else {
        let mut list = column![].spacing(12);
        for group in visible_groups(groups, show_inactive) {
            list = list.push(group_section(group, streams, show_inactive));
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

/// Groups worth rendering: when inactive clients are hidden, a group whose
/// clients are *all* disconnected has nothing left to show, so drop the
/// whole group rather than leaving an empty card behind.
fn visible_groups(groups: &[SnapGroup], show_inactive: bool) -> Vec<&SnapGroup> {
    groups
        .iter()
        .filter(|g| show_inactive || g.clients.iter().any(|c| c.connected))
        .collect()
}

fn group_section<'a>(
    group: &'a SnapGroup,
    streams: &'a [crate::snapcast::SnapStream],
    show_inactive: bool,
) -> Element<'a, Message> {
    let mute_label = if group.muted { "Unmute Group" } else { "Mute Group" };
    let group_id = group.id.clone();
    let was_muted = group.muted;

    let mut header = row![
        text(group.display_name()).size(16).color(AppColors::text_primary()),
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
    for client in group.clients.iter().filter(|c| show_inactive || c.connected) {
        client_rows = client_rows.push(client_row(client));
    }

    container(column![header, Space::with_height(10), client_rows].spacing(0))
        .padding(12)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(AppColors::bg_secondary().into()),
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
        AppColors::success()
    } else {
        AppColors::text_muted()
    };
    let name_color = if client.connected {
        AppColors::text_primary()
    } else {
        AppColors::text_muted()
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
        icon::icon_sized(icon::DOT, 10).color(dot_color),
        text(client.display_name().to_string())
            .size(13)
            .color(name_color)
            .width(140),
        text(latency_label).size(10).color(AppColors::text_muted()).width(50),
        slider(0u8..=100u8, client.volume, move |v| {
            Message::SnapcastSetVolume(slider_id.clone(), v)
        })
        .width(140)
        .step(1u8),
        text(format!("{}%", client.volume))
            .size(12)
            .color(AppColors::text_muted())
            .width(36),
        button(text(mute_label).size(11))
            .on_press(Message::SnapcastToggleClientMute(mute_id, was_muted))
            .padding([3, 10]),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
