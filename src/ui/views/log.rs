//! In-app log view — shows winrmpc tracing events newest-first.

use crate::logger::LogEntry;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length};

pub fn view<'a>(entries: &'a [LogEntry], show_mpd_only: bool) -> Element<'a, Message> {
    let title = text("Log")
        .size(24)
        .color(AppColors::TEXT_PRIMARY);

    let toggle_label = if show_mpd_only { "MPD only ✓" } else { "All logs" };
    let toggle_btn = button(text(toggle_label).size(12))
        .on_press(Message::LogToggleMpdOnly)
        .padding([4, 12]);

    let copy_btn = button(text("Copy").size(12))
        .on_press(Message::LogCopyAll)
        .padding([4, 12]);

    let clear_btn = button(text("Clear").size(12))
        .on_press(Message::LogClear)
        .padding([4, 12]);

    let header = row![
        title,
        Space::with_width(Length::Fill),
        toggle_btn,
        Space::with_width(8),
        copy_btn,
        Space::with_width(8),
        clear_btn,
    ]
    .align_y(iced::Alignment::Center);

    // Apply filter
    let displayed: Vec<&LogEntry> = if show_mpd_only {
        entries.iter().filter(|e| e.target.contains("mpd")).collect()
    } else {
        entries.iter().collect()
    };

    let mut log_col = column![].spacing(0);

    if displayed.is_empty() {
        let msg = if show_mpd_only && !entries.is_empty() {
            "No MPD log entries yet."
        } else {
            "No log entries yet."
        };
        log_col = log_col.push(
            container(
                text(msg)
                    .size(12)
                    .color(AppColors::TEXT_MUTED),
            )
            .padding([3, 8]),
        );
    } else {
        for (i, entry) in displayed.iter().rev().enumerate() {
            let level_color = match entry.level.as_str() {
                "ERROR" => AppColors::ERROR,
                "WARN"  => AppColors::WARNING,
                "INFO"  => AppColors::TEXT_PRIMARY,
                _       => AppColors::TEXT_MUTED,
            };

            // Strip the crate prefix so "winrmpc::mpd::client" → "mpd::client"
            let target = entry.target
                .strip_prefix("winrmpc::")
                .unwrap_or(&entry.target);

            let line = text(format!(
                "{} {:5} {}  {}",
                entry.timestamp, entry.level, target, entry.message
            ))
            .size(11)
            .color(level_color)
            .font(iced::Font::MONOSPACE);

            let bg = if i % 2 == 0 {
                AppColors::ROW_EVEN
            } else {
                AppColors::ROW_ODD
            };

            log_col = log_col.push(
                container(line)
                    .padding([3, 8])
                    .width(Length::Fill)
                    .style(move |_theme: &iced::Theme| container::Style {
                        background: Some(bg.into()),
                        ..Default::default()
                    }),
            );
        }
    }

    let content = column![
        header,
        Space::with_height(12),
        scrollable(log_col).height(Length::Fill),
    ]
    .padding(20)
    .width(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
