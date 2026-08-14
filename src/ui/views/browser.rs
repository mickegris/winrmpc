use crate::mpd::types::*;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::ui::widgets::link::icon_btn_tip;
use crate::ui::widgets::song_row;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

/// `current_file` marks the playing track among `DirectoryEntry::File` rows
/// only — directories and playlists have no track identity.
pub fn view<'a>(
    current_path: &'a str,
    entries: &'a [DirectoryEntry],
    current_file: Option<&'a str>,
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

    // Action buttons for current directory
    let has_files = entries.iter().any(|e| matches!(e, DirectoryEntry::File(_)));
    let dir_path = current_path.to_string();

    let action_buttons = if has_files && !current_path.is_empty() {
        row![
            button(text("Play All").size(12))
                .on_press(Message::QueueAddAndPlay(dir_path.clone()))
                .padding([4, 12]),
            Space::with_width(4),
            button(text("Queue All").size(12))
                .on_press(Message::QueueAddOnly(dir_path))
                .padding([4, 12]),
        ]
        .spacing(4)
    } else {
        row![]
    };

    let mut items = column![].spacing(0);
    for (i, entry) in entries.iter().enumerate() {
        let is_current = match entry {
            DirectoryEntry::File(s) => song_row::is_current_uri(&s.file, current_file),
            _ => false,
        };
        let bg = song_row::row_bg(i, is_current);

        match entry {
            DirectoryEntry::File(s) => {
                // File rows carry the same four actions as the album, search and
                // playlist track lists — see the row-action table in CLAUDE.md.
                let label = format!("{} – {}", s.display_artist(), s.display_title());
                let duration = s.format_duration();
                let file_uri = s.file.clone();
                items = items.push(
                    container(
                        row![
                            song_row::playing_marker(is_current),
                            icon_btn_tip(
                                icon::PLAY,
                                "Play now",
                                Message::PlaySong(file_uri.clone())
                            ),
                            icon_btn_tip(
                                icon::ADD_QUEUE,
                                "Add to end of queue",
                                Message::QueueAddOnly(file_uri.clone())
                            ),
                            icon_btn_tip(
                                icon::PLAY_NEXT,
                                "Play next",
                                Message::QueueAddNext(file_uri.clone())
                            ),
                            icon_btn_tip(
                                icon::ADD_PLAYLIST,
                                "Add to playlist…",
                                Message::OpenAddToPlaylist(vec![file_uri])
                            ),
                            text(label)
                                .size(13)
                                .color(song_row::title_color(is_current))
                                .width(Length::Fill),
                            text(duration)
                                .size(11)
                                .color(AppColors::TEXT_MUTED),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([6, 12])
                    .width(Length::Fill)
                    .style(move |_theme: &iced::Theme| container::Style {
                        background: Some(bg.into()),
                        ..Default::default()
                    }),
                );
            }
            _ => {
                let (prefix, label, action) = match entry {
                    DirectoryEntry::Directory(d) => {
                        let name = d.path.rsplit('/').next().unwrap_or(&d.path);
                        ("[dir]", name.to_string(), Message::BrowsePath(d.path.clone()))
                    }
                    DirectoryEntry::Playlist(p) => {
                        ("[list]", p.name.clone(), Message::PlaylistSelected(p.name.clone()))
                    }
                    DirectoryEntry::File(_) => unreachable!(),
                };
                let prefix_color = match entry {
                    DirectoryEntry::Directory(_) => AppColors::ACCENT,
                    _ => AppColors::SUCCESS,
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
        }
    }

    container(
        column![
            container(
                column![
                    text("Browse").size(24).color(AppColors::TEXT_PRIMARY),
                    Space::with_height(8),
                    breadcrumb,
                    Space::with_height(8),
                    action_buttons,
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
