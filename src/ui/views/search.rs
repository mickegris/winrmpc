use crate::mpd::types::Song;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Alignment, Element, Length};
use std::collections::BTreeMap;

pub fn view<'a>(
    query: &'a str,
    results: &'a [Song],
) -> Element<'a, Message> {
    let search_bar = row![
        text_input("Search your library...", query)
            .on_input(Message::SearchQueryChanged)
            .on_submit(Message::SearchSubmit)
            .size(16)
            .padding(10)
            .width(Length::Fill),
        button(text("Search").size(14))
            .on_press(Message::SearchSubmit)
            .padding([10, 20]),
    ]
    .spacing(8);

    let mut by_album: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, song) in results.iter().enumerate() {
        let album_key = song.display_album().to_string();
        by_album.entry(album_key).or_default().push(i);
    }

    for indices in by_album.values_mut() {
        indices.sort_by(|&a, &b| {
            let ta: u32 = results[a].track.as_deref().and_then(|t| t.parse().ok()).unwrap_or(0);
            let tb: u32 = results[b].track.as_deref().and_then(|t| t.parse().ok()).unwrap_or(0);
            ta.cmp(&tb)
        });
    }

    let album_count = by_album.len();
    let mut row_index = 0usize;

    let mut result_list = column![].spacing(0);
    for (album, indices) in &by_album {
        let artist = indices
            .first()
            .map(|&i| results[i].display_album_artist())
            .unwrap_or("Unknown Artist");

        result_list = result_list.push(
            button(
                row![
                    text(album.clone())
                        .size(13)
                        .color(AppColors::ACCENT),
                    Space::with_width(8),
                    text(format!("by {artist}"))
                        .size(11)
                        .color(AppColors::TEXT_MUTED),
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Message::AlbumSelected(album.clone()))
            .padding([8, 12])
            .width(Length::Fill)
            .style(|_theme: &iced::Theme, _status| button::Style {
                background: Some(AppColors::BG_TERTIARY.into()),
                text_color: AppColors::TEXT_PRIMARY,
                border: iced::Border::default(),
                ..Default::default()
            }),
        );

        for &idx in indices {
            let song = &results[idx];
            let track = song.track.as_deref().unwrap_or("-");
            let bg = if row_index % 2 == 0 {
                AppColors::ROW_EVEN
            } else {
                AppColors::ROW_ODD
            };
            row_index += 1;

            result_list = result_list.push(
                button(
                    row![
                        Space::with_width(16),
                        text(track.to_string())
                            .size(12)
                            .width(30)
                            .color(AppColors::TEXT_MUTED),
                        text(song.display_title())
                            .size(12)
                            .color(AppColors::TEXT_PRIMARY)
                            .width(Length::Fill),
                        text(song.display_artist())
                            .size(11)
                            .color(AppColors::TEXT_SECONDARY)
                            .width(Length::FillPortion(2)),
                        text(song.format_duration())
                            .size(11)
                            .color(AppColors::TEXT_MUTED),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SearchAddToQueue(song.file.clone()))
                .padding([4, 12])
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

    let status_text = if !query.is_empty() {
        format!("{} songs in {} albums", results.len(), album_count)
    } else {
        "Type to search".into()
    };

    container(
        column![
            container(
                column![
                    text("Search").size(24).color(AppColors::TEXT_PRIMARY),
                    Space::with_height(12),
                    search_bar,
                    Space::with_height(8),
                    text(status_text).size(13).color(AppColors::TEXT_MUTED),
                ]
            )
            .padding(12),
            scrollable(result_list).height(Length::Fill),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
