use crate::mpd::types::*;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    current_path: &'a str,
    entries: &'a [DirectoryEntry],
) -> Element<'a, Message> {
    let breadcrumb = {
        let parts: Vec<&str> = if current_path.is_empty() {
            vec!["Root"]
        } else {
            std::iter::once("Root")
                .chain(current_path.split('/'))
                .collect()
        };
        let mut r = row![].spacing(4);
        let mut path_so_far = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                r = r.push(text(" / ").size(14).color(AppColors::TEXT_MUTED));
                if i == 1 {
                    path_so_far = part.to_string();
                } else {
                    path_so_far = format!("{path_so_far}/{part}");
                }
            }
            let target = if i == 0 {
                String::new()
            } else {
                path_so_far.clone()
            };
            r = r.push(
                button(text(*part).size(14).color(AppColors::ACCENT))
                    .on_press(Message::BrowsePath(target))
                    .padding([2, 4])
                    .style(|_theme: &iced::Theme, _status| button::Style {
                        background: None,
                        text_color: AppColors::ACCENT,
                        border: iced::Border::default(),
                        ..Default::default()
                    }),
            );
        }
        r
    };

    let mut items = column![].spacing(0);
    for (i, entry) in entries.iter().enumerate() {
        let (prefix, label, action) = match entry {
            DirectoryEntry::Directory(d) => {
                let name = d.path.rsplit('/').next().unwrap_or(&d.path);
                ("[dir]", name.to_string(), Message::BrowsePath(d.path.clone()))
            }
            DirectoryEntry::File(s) => {
                let label = format!(
                    "{} - {}",
                    s.display_artist(),
                    s.display_title()
                );
                ("[file]", label, Message::BrowseAddToQueue(s.file.clone()))
            }
            DirectoryEntry::Playlist(p) => {
                ("[list]", p.name.clone(), Message::QueueAddUri(p.name.clone()))
            }
        };

        let prefix_color = match entry {
            DirectoryEntry::Directory(_) => AppColors::ACCENT,
            DirectoryEntry::File(_) => AppColors::TEXT_MUTED,
            DirectoryEntry::Playlist(_) => AppColors::SUCCESS,
        };

        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        items = items.push(
            button(
                row![
                    text(prefix).size(11).width(40).color(prefix_color),
                    text(label).size(13).color(AppColors::TEXT_PRIMARY),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(action)
            .padding([6, 12])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme, _status| button::Style {
                background: Some(bg.into()),
                text_color: AppColors::TEXT_PRIMARY,
                border: iced::Border::default(),
                ..Default::default()
            }),
        );
    }

    container(
        column![
            container(
                column![
                    text("Browse").size(24).color(AppColors::TEXT_PRIMARY),
                    Space::with_height(8),
                    breadcrumb,
                ]
            )
            .padding(12),
            scrollable(items).height(Length::Fill),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
