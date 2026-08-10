//! Recently Played history — Albums/Tracks toggle over per-server listening
//! history. Albums are derived from track history (not recorded
//! separately), mirroring mikMPD's `recentAlbumGroups`.

use crate::mpd::types::{recently_played_albums, relative_time, RecentlyPlayedEntry};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, image, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;

const TILE_SIZE: u16 = 120;
const TILES_PER_ROW: usize = 4;

pub fn view<'a>(
    entries: &'a [RecentlyPlayedEntry],
    show_albums: bool,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
) -> Element<'a, Message> {
    let mode_label = if show_albums { "Albums" } else { "Tracks" };
    let toggle_btn = button(text(format!("View: {mode_label}")).size(12))
        .on_press(Message::ToggleRecentlyPlayedMode)
        .padding([4, 12]);

    let clear_btn = button(text("Clear").size(12))
        .on_press(Message::ClearRecentlyPlayed)
        .padding([4, 12]);

    let header = row![
        button(text("<- Back").size(14).color(AppColors::ACCENT))
            .on_press(Message::GoBack)
            .padding([4, 8]),
        Space::with_width(12),
        text("Recently Played").size(24).color(AppColors::TEXT_PRIMARY),
        Space::with_width(Length::Fill),
        toggle_btn,
        Space::with_width(8),
        clear_btn,
    ]
    .align_y(Alignment::Center)
    .padding([12, 12]);

    let body: Element<'a, Message> = if entries.is_empty() {
        container(
            text("Nothing played yet.")
                .size(14)
                .color(AppColors::TEXT_MUTED),
        )
        .padding(20)
        .into()
    } else if show_albums {
        let now = chrono::Utc::now().timestamp();
        let groups = recently_played_albums(entries);
        let tiles: Vec<Element<'a, Message>> = groups
            .into_iter()
            .map(|g| album_tile(g, now, art_handles))
            .collect();

        // `Element` isn't `Clone`, so chunk by consuming the iterator in
        // fixed-size groups rather than slicing borrowed `tiles`.
        let mut rows = column![].spacing(16);
        let mut iter = tiles.into_iter();
        loop {
            let group: Vec<Element<'a, Message>> = (&mut iter).take(TILES_PER_ROW).collect();
            if group.is_empty() {
                break;
            }
            rows = rows.push(row(group).spacing(16));
        }
        scrollable(container(rows).padding(20)).height(Length::Fill).into()
    } else {
        let now = chrono::Utc::now().timestamp();
        let mut list = column![].spacing(0);
        for (i, e) in entries.iter().enumerate() {
            let bg = if i % 2 == 0 {
                AppColors::ROW_EVEN
            } else {
                AppColors::ROW_ODD
            };
            list = list.push(
                container(
                    button(
                        row![
                            text(e.title.as_str())
                                .size(13)
                                .width(Length::FillPortion(3))
                                .color(AppColors::TEXT_PRIMARY),
                            text(e.artist.as_str())
                                .size(12)
                                .width(Length::FillPortion(2))
                                .color(AppColors::TEXT_SECONDARY),
                            text(relative_time(now - e.played_at))
                                .size(11)
                                .color(AppColors::TEXT_MUTED),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::PlaySong(e.file.clone()))
                    .padding(0)
                    .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                        background: None,
                        text_color: AppColors::TEXT_PRIMARY,
                        border: iced::Border::default(),
                        shadow: iced::Shadow::default(),
                    }),
                )
                .padding([6, 12])
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    ..Default::default()
                }),
            );
        }
        scrollable(list).height(Length::Fill).into()
    };

    container(column![header, body].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn album_tile<'a>(
    group: crate::mpd::types::RecentlyPlayedAlbum,
    now: i64,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
) -> Element<'a, Message> {
    let key = format!("{}\x1f{}", group.artist, group.album);
    let art: Element<'a, Message> = match art_handles.get(&key) {
        Some(handle) => image(handle.clone()).width(TILE_SIZE).height(TILE_SIZE).into(),
        None => container(text("").size(1))
            .width(TILE_SIZE)
            .height(TILE_SIZE)
            .style(|_t: &iced::Theme| container::Style {
                background: Some(AppColors::BG_SECONDARY.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
    };

    let crate::mpd::types::RecentlyPlayedAlbum { artist, album, last_played } = group;
    let relative = relative_time(now - last_played);

    button(
        column![
            art,
            Space::with_height(6),
            text(album.clone()).size(13).color(AppColors::TEXT_PRIMARY),
            text(artist).size(11).color(AppColors::TEXT_SECONDARY),
            text(relative).size(10).color(AppColors::TEXT_MUTED),
        ]
        .align_x(Alignment::Center)
        .width(TILE_SIZE),
    )
    .on_press(Message::AlbumSelected(album))
    .padding(4)
    .style(|_t: &iced::Theme, status: button::Status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(AppColors::BG_HOVER.into())
            }
            _ => None,
        };
        button::Style {
            background: bg,
            text_color: AppColors::TEXT_PRIMARY,
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            shadow: iced::Shadow::default(),
        }
    })
    .into()
}
