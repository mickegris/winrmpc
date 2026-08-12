//! Server Statistics view — library totals, uptime/playtime, and a
//! database update/rescan trigger. Mirrors mikMPD's "More" tab statistics
//! screen.

use crate::mpd::types::Stats;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length};
use std::time::Duration;

pub fn view<'a>(stats: Option<&'a Stats>, updating: bool) -> Element<'a, Message> {
    let header = row![
        button(text("<- Back").size(14).color(AppColors::ACCENT))
            .on_press(Message::GoBack)
            .padding([4, 8]),
        Space::with_width(12),
        text("Server Statistics").size(24).color(AppColors::TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center)
    .padding([12, 12]);

    let body: Element<'a, Message> = match stats {
        None => container(
            text("Loading...").size(14).color(AppColors::TEXT_MUTED),
        )
        .padding(20)
        .into(),
        Some(s) => {
            let mut col = column![].spacing(10).padding(20);
            col = col.push(stat_row("Songs", s.songs.to_string()));
            col = col.push(stat_row("Albums", s.albums.to_string()));
            col = col.push(stat_row("Artists", s.artists.to_string()));
            col = col.push(stat_row("Server uptime", format_hms(s.uptime)));
            col = col.push(stat_row("Play time (this session)", format_hms(s.playtime)));
            col = col.push(stat_row("Total library play time", format_hms(s.db_playtime)));
            col = col.push(stat_row("Last database update", format_timestamp(s.db_update)));
            col = col.push(Space::with_height(10));
            col = col.push(
                text("Rescan your MPD music directory for new or changed files.")
                    .size(12)
                    .color(AppColors::TEXT_SECONDARY),
            );
            col = col.push(Space::with_height(4));
            let update_label = if updating { "Updating..." } else { "Update Database" };
            col = col.push(
                button(text(update_label).size(14))
                    .on_press_maybe((!updating).then_some(Message::UpdateDatabase))
                    .padding([8, 20]),
            );
            col.into()
        }
    };

    container(column![header, body].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn stat_row<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![
        text(label).size(13).color(AppColors::TEXT_MUTED).width(220),
        text(value).size(14).color(AppColors::TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn format_hms(d: Duration) -> String {
    let total = d.as_secs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3600;
    let mins = (total % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

fn format_timestamp(unix_secs: u64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix_secs as i64, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        _ => "Unknown".to_string(),
    }
}
