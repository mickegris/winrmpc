//! In-app log view — shows winrmpc tracing events newest-first.

use crate::logger::LogEntry;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(entries: &'a [LogEntry], show_mpd_only: bool) -> Element<'a, Message> {
    let title = text("Log")
        .size(24)
        .color(AppColors::text_primary());

    let toggle_inner: Element<'_, Message> = if show_mpd_only {
        row![
            text("MPD only").size(12),
            icon::icon_sized(icon::CHECK, 13),
        ]
        .spacing(5)
        .align_y(Alignment::Center)
        .into()
    } else {
        text("All logs").size(12).into()
    };
    let toggle_btn = button(toggle_inner)
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
                    .color(AppColors::text_muted()),
            )
            .padding([3, 8]),
        );
    } else {
        for (i, entry) in displayed.iter().rev().enumerate() {
            let slow = entry.is_slow();
            let level_color = if slow {
                AppColors::warning()
            } else {
                match entry.level.as_str() {
                    "ERROR" => AppColors::error(),
                    "WARN"  => AppColors::warning(),
                    "INFO"  => AppColors::text_primary(),
                    _       => AppColors::text_muted(),
                }
            };

            // Strip the crate prefix so "winrmpc::mpd::client" → "mpd::client"
            let target = entry.target
                .strip_prefix("winrmpc::")
                .unwrap_or(&entry.target);

            let message = text(format!(
                "{} {:5} {}  {}",
                entry.timestamp, entry.level, target, entry.message
            ))
            .size(11)
            .color(level_color)
            .font(iced::Font::MONOSPACE);

            // The slow-command marker has to be its own widget: the log line is
            // monospace and the marker comes from the icon font, and one `text`
            // can only carry one font. The fixed-width cell keeps every line's
            // text starting at the same x, marked or not.
            let marker: Element<'_, Message> = if slow {
                icon::icon_sized(icon::WARNING, 12)
                    .color(AppColors::warning())
                    .into()
            } else {
                Space::with_width(0).into()
            };
            let line = row![container(marker).width(16), message]
                .spacing(2)
                .align_y(Alignment::Center);

            let bg = if i % 2 == 0 {
                AppColors::row_even()
            } else {
                AppColors::row_odd()
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
