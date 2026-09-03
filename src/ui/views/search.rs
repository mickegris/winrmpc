use crate::mpd::types::{AlbumGroup, SearchSections, Song};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::album_grid;
use crate::ui::widgets::icon;
use crate::ui::widgets::link;
use crate::ui::widgets::link::icon_btn_tip;
use crate::ui::widgets::song_row;
use iced::widget::{
    button, checkbox, column, container, row, scrollable, text, text_input, Space,
};
use iced::{Alignment, Element, Length};
use std::collections::{HashMap, HashSet};

/// The search box's id, so `Ctrl+F` / `/` can focus it from anywhere.
///
/// A stable id rather than `Id::unique()`: the whole point is that another
/// module can name this widget.
pub fn input_id() -> iced::widget::text_input::Id {
    iced::widget::text_input::Id::new("winrmpc-search-input")
}

/// A section heading. Sections are only drawn when they have content, so an
/// artist-less query doesn't leave an empty "Artists" label behind.
fn heading<'a>(label: String) -> Element<'a, Message> {
    container(text(label).size(12).color(AppColors::text_muted()))
        .padding([10, 12])
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppColors::bg_tertiary().into()),
            ..Default::default()
        })
        .into()
}

/// One row in the Albums section: cover, title, artist — laid out like the
/// album lists' list mode, and navigating through the same shared message
/// builder so it can't drift from them.
fn album_row<'a>(
    group: &'a AlbumGroup,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
    index: usize,
) -> Element<'a, Message> {
    let caption: Element<'a, Message> = match group.disc_caption() {
        Some(c) => text(c).size(11).color(AppColors::text_muted()).into(),
        None => Space::with_width(0).into(),
    };

    let bg = song_row::row_bg(index, false);
    container(
        row![
            album_grid::list_thumb(album_grid::art_for(art_handles, &group.artist, &group.base)),
            // Cover + title are one target; the artist is a sibling link, so
            // a name link is never nested inside a bigger button.
            button(
                text(group.base.clone())
                    .size(13)
                    .color(AppColors::text_primary())
            )
            .on_press(link::album_message(&group.base, Some(&group.artist)))
            .padding(0)
            .width(Length::FillPortion(3))
            .style(|_t: &iced::Theme, _s| button::Style {
                background: None,
                text_color: AppColors::text_primary(),
                border: iced::Border::default(),
                ..Default::default()
            }),
            container(link::artist_link(&group.artist, 11)).width(Length::FillPortion(2)),
            caption,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([4, 12])
    .width(Length::Fill)
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(bg.into()),
        ..Default::default()
    })
    .into()
}

pub fn view<'a>(
    query: &'a str,
    results: &'a [Song],
    sections: &'a SearchSections,
    selected: &'a HashSet<String>,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
    current_file: Option<&'a str>,
) -> Element<'a, Message> {
    let search_bar = row![
        text_input("Search your library...", query)
            .id(input_id())
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

    let mut result_list = column![].spacing(0);

    // --- Artists -----------------------------------------------------------
    if !sections.artists.is_empty() {
        result_list = result_list.push(heading(format!("Artists ({})", sections.artists.len())));
        for (i, name) in sections.artists.iter().enumerate() {
            let bg = song_row::row_bg(i, false);
            result_list = result_list.push(
                container(
                    row![
                        Space::with_width(8),
                        link::artist_link(name, 13),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([6, 12])
                .width(Length::Fill)
                .style(move |_t: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    ..Default::default()
                }),
            );
        }
    }

    // --- Albums ------------------------------------------------------------
    if !sections.albums.is_empty() {
        result_list = result_list.push(heading(format!("Albums ({})", sections.albums.len())));
        for (i, group) in sections.albums.iter().enumerate() {
            result_list = result_list.push(album_row(group, art_handles, i));
        }
    }

    // --- Songs -------------------------------------------------------------
    if !results.is_empty() {
        let all_selected = selected.len() == results.len();
        result_list = result_list.push(
            container(
                row![
                    checkbox("", all_selected)
                        .on_toggle(Message::SearchSelectAll)
                        .size(14),
                    text(format!("Songs ({})", results.len()))
                        .size(12)
                        .color(AppColors::text_muted()),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([10, 12])
            .width(Length::Fill)
            .style(|_t: &iced::Theme| container::Style {
                background: Some(AppColors::bg_tertiary().into()),
                ..Default::default()
            }),
        );

        for (i, song) in results.iter().enumerate() {
            let track = song.track.as_deref().unwrap_or("-");
            let is_current = song_row::is_current_uri(&song.file, current_file);
            let bg = song_row::row_bg(i, is_current);
            let is_selected = selected.contains(&song.file);

            let actions = row![
                icon_btn_tip(
                    icon::ADD_QUEUE,
                    "Add to end of queue",
                    Message::QueueAddOnly(song.file.clone())
                ),
                icon_btn_tip(
                    icon::PLAY_NEXT,
                    "Play next",
                    Message::QueueAddNext(song.file.clone())
                ),
                icon_btn_tip(
                    icon::ADD_PLAYLIST,
                    "Add to playlist…",
                    Message::OpenAddToPlaylist(vec![song.file.clone()])
                ),
            ]
            .spacing(song_row::ACTION_SPACING)
            .width(song_row::action_group_width(3));

            result_list = result_list.push(
                container(
                    row![
                        checkbox("", is_selected)
                            .on_toggle({
                                let uri = song.file.clone();
                                move |_| Message::SearchToggleSelected(uri.clone())
                            })
                            .size(14),
                        song_row::playing_marker(is_current),
                        icon_btn_tip(
                            icon::PLAY,
                            "Play now",
                            Message::PlaySong(song.file.clone())
                        ),
                        song_row::number(track.to_string(), 12),
                        text(song.display_title())
                            .size(12)
                            .color(song_row::title_color(is_current))
                            .width(Length::Fill),
                        container(link::artist_link(song.display_artist(), 11))
                            .width(Length::FillPortion(2)),
                        container(link::album_link(
                            song.display_album(),
                            Some(song.display_album_artist()),
                            11
                        ))
                        .width(Length::FillPortion(2)),
                        song_row::duration(song.format_duration(), 11),
                        actions,
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([4, 12])
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    ..Default::default()
                }),
            );
        }
    }

    let status_text = if query.is_empty() {
        "Type to search".to_string()
    } else {
        format!(
            "{} songs · {} albums · {} artists",
            results.len(),
            sections.albums.len(),
            sections.artists.len()
        )
    };

    // The batch bar appears only when something is ticked: an always-present
    // row of disabled buttons is noise on every search that doesn't use them.
    let header: Element<'a, Message> = if selected.is_empty() {
        text(status_text).size(13).color(AppColors::text_muted()).into()
    } else {
        let uris: Vec<String> = results
            .iter()
            .filter(|s| selected.contains(&s.file))
            .map(|s| s.file.clone())
            .collect();
        row![
            text(format!("{} selected", selected.len()))
                .size(13)
                .color(AppColors::accent()),
            button(text("Add to queue").size(12))
                .on_press(Message::SearchQueueSelected)
                .padding([6, 12]),
            button(text("Add to playlist…").size(12))
                .on_press(Message::OpenAddToPlaylist(uris))
                .padding([6, 12]),
            button(text("Clear").size(12))
                .on_press(Message::SearchSelectAll(false))
                .padding([6, 12]),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };

    container(
        column![
            container(
                column![
                    text("Search").size(24).color(AppColors::text_primary()),
                    Space::with_height(12),
                    search_bar,
                    Space::with_height(8),
                    header,
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
