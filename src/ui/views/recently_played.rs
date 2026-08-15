//! Recently Played history — Albums/Tracks toggle over per-server listening
//! history. Albums are derived from track history (not recorded
//! separately), mirroring mikMPD's `recentAlbumGroups`.

use crate::mpd::types::{art_key_for, recently_played_albums, relative_time, RecentlyPlayedEntry};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::album_grid;
use crate::ui::widgets::link;
use crate::ui::widgets::song_row;
use iced::widget::{button, column, container, image, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;


/// `current_file` marks the playing track in **Tracks** mode. Albums mode is
/// album-level state, which is a different comparison (see plan step D) and is
/// deliberately not done here.
pub fn view<'a>(
    entries: &'a [RecentlyPlayedEntry],
    show_albums: bool,
    grid_view: bool,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
    current_file: Option<&'a str>,
    current_song: Option<&'a crate::mpd::types::Song>,
) -> Element<'a, Message> {
    let mode_label = if show_albums { "Albums" } else { "Tracks" };
    let toggle_btn = button(text(format!("View: {mode_label}")).size(12))
        .on_press(Message::ToggleRecentlyPlayedMode)
        .padding([4, 12]);

    let clear_btn = button(text("Clear").size(12))
        .on_press(Message::ClearRecentlyPlayed)
        .padding([4, 12]);

    let header = row![
        link::back_button(),
        Space::with_width(12),
        text("Recently Played").size(24).color(AppColors::TEXT_PRIMARY),
        Space::with_width(Length::Fill),
        toggle_btn,
        Space::with_width(8),
    ]
    .align_y(Alignment::Center)
    .padding([12, 12]);
    // Grid/list only applies to the Albums mode; the Tracks mode is
    // inherently a list.
    let header = if show_albums {
        header.push(album_grid::layout_toggle(grid_view)).push(Space::with_width(8))
    } else {
        header
    };
    let header = header.push(clear_btn);

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
        if grid_view {
            let tiles: Vec<Element<'a, Message>> = groups
                .into_iter()
                .map(|g| {
                    let art = album_grid::art_for(art_handles, &g.artist, &g.album);
                    let caption = relative_time(now - g.last_played);
                    album_grid::tile(
                        art,
                        g.album.clone(),
                        g.artist.clone(),
                        Some(caption),
                        song_row::is_current_album(&g.artist, &g.album, current_song),
                    )
                })
                .collect();
            scrollable(container(album_grid::grid(tiles)).padding(20))
                .height(Length::Fill)
                .into()
        } else {
            let mut list = column![].spacing(0);
            for (i, g) in groups.into_iter().enumerate() {
                let bg = if i % 2 == 0 { AppColors::ROW_EVEN } else { AppColors::ROW_ODD };
                // Same cover as this view's grid mode, just small — matching
                // Albums and Recently Added, whose list modes already do this.
                let album_btn = button(
                    row![
                        album_grid::list_thumb(album_grid::art_for(
                            art_handles,
                            &g.artist,
                            &g.album
                        )),
                        text(g.album.clone()).size(14).color(song_row::title_color(
                            song_row::is_current_album(&g.artist, &g.album, current_song),
                        )),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .on_press(link::album_message(&g.album, Some(&g.artist)))
                .padding(0)
                .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                    background: None,
                    text_color: AppColors::TEXT_PRIMARY,
                    border: iced::Border::default(),
                    ..Default::default()
                });

                let label = row![
                    album_btn,
                    link::artist_link(&g.artist, 12),
                    Space::with_width(Length::Fill),
                    text(relative_time(now - g.last_played))
                        .size(11)
                        .color(AppColors::TEXT_MUTED),
                ]
                .spacing(8)
                .align_y(Alignment::Center);
                list = list.push(
                    container(label)
                        .padding([7, 12])
                        .width(Length::Fill)
                        .style(move |_t: &iced::Theme| container::Style {
                            background: Some(bg.into()),
                            ..Default::default()
                        }),
                );
            }
            scrollable(list).height(Length::Fill).into()
        }
    } else {
        let now = chrono::Utc::now().timestamp();
        let mut list = column![].spacing(0);
        for (i, e) in entries.iter().enumerate() {
            let is_current = song_row::is_current_uri(&e.file, current_file);
            let bg = song_row::row_bg(i, is_current);
            list = list.push(
                container(
                    row![
                        song_row::playing_marker(is_current),
                        button(
                            text(e.title.as_str())
                                .size(13)
                                .color(song_row::title_color(is_current)),
                        )
                        .on_press(Message::PlaySong(e.file.clone()))
                        .padding(0)
                        .width(Length::FillPortion(3))
                        .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                            background: None,
                            text_color: AppColors::TEXT_PRIMARY,
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                        container(link::artist_link(&e.artist, 12))
                            .width(Length::FillPortion(2)),
                        text(relative_time(now - e.played_at))
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
        scrollable(list).height(Length::Fill).into()
    };

    container(column![header, body].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

