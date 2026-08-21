use crate::mpd::types::{AlbumGroup, Song, SortKey};
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::album_grid;
use crate::ui::widgets::link;
use crate::ui::widgets::song_row;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;

pub fn view<'a>(
    albums: &'a [AlbumGroup],
    title: &'a str,
    // A note under the count, e.g. the window Recently Added covers. Without
    // it a list bounded by a query looks the same as a list that is simply
    // short.
    subtitle: Option<String>,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
    grid_view: bool,
    current_song: Option<&'a Song>,
    // `None` where the list isn't name- or add-time-sorted at all (Recently
    // Added, which is ordered by time and only by time). An Option rather
    // than a plain pair because the control must be *absent* there, not
    // present and doing nothing.
    sort: Option<(SortKey, bool)>,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if grid_view {
        let tiles: Vec<Element<'a, Message>> = albums
            .iter()
            .map(|group| {
                album_grid::tile(
                    album_grid::art_for(art_handles, &group.artist, &group.base),
                    group.base.clone(),
                    group.artist.clone(),
                    (group.variants.len() > 1)
                        .then(|| format!("{} discs", group.variants.len())),
                    song_row::is_current_album(&group.artist, &group.base, current_song),
                )
            })
            .collect();
        scrollable(container(album_grid::grid(tiles)).padding(20))
            .height(Length::Fill)
            .into()
    } else {
        let mut list = column![].spacing(0);
        for (i, group) in albums.iter().enumerate() {
            let bg = if i % 2 == 0 {
                AppColors::row_even()
            } else {
                AppColors::row_odd()
            };

            // Same cover as the grid, just small — so switching layouts
            // doesn't change which albums appear to have art.
            let thumb = album_grid::list_thumb(album_grid::art_for(
                art_handles,
                &group.artist,
                &group.base,
            ));

            // Thumb + album title are one button; the artist beside them is
            // its own link. Same reasoning as `album_grid::tile` — the artist
            // must not be a small region of a bigger button that goes
            // somewhere else.
            let is_current =
                song_row::is_current_album(&group.artist, &group.base, current_song);
            let album_btn = button(
                row![
                    thumb,
                    text(group.base.as_str())
                        .size(14)
                        .color(song_row::title_color(is_current)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(link::album_message(&group.base, artist_opt(group).as_deref()))
            .padding(0)
            .style(|_theme: &iced::Theme, _status| button::Style {
                background: None,
                text_color: AppColors::text_primary(),
                border: iced::Border::default(),
                ..Default::default()
            });

            let mut label = row![album_btn].spacing(8).align_y(Alignment::Center);

            if !group.artist.is_empty() {
                label = label.push(link::artist_link(&group.artist, 12));
            }
            if group.variants.len() > 1 {
                label = label.push(
                    text(format!("{} discs", group.variants.len()))
                        .size(11)
                        .color(AppColors::accent()),
                );
            }

            list = list.push(
                container(label)
                    .padding([7, 12])
                    .width(Length::Fill)
                    .style(move |_theme: &iced::Theme| container::Style {
                        background: Some(bg.into()),
                        ..Default::default()
                    }),
            );
        }
        scrollable(list).height(Length::Fill).into()
    };

    container(
        column![
            row![
                link::back_button(),
                Space::with_width(12),
                text(title).size(24).color(AppColors::text_primary()),
                Space::with_width(12),
                text(format!("{} albums", albums.len()))
                    .size(14)
                    .color(AppColors::text_muted()),
                Space::with_width(8),
                text(subtitle.unwrap_or_default())
                    .size(13)
                    .color(AppColors::text_muted()),
                Space::with_width(Length::Fill),
                // `None` on the recency lists: Recently Added is ordered by
                // time, which is the only thing it's for, so the control is
                // absent rather than present-and-ignored.
                match sort {
                    Some((key, desc)) => link::album_sort_controls(key, desc),
                    None => Space::with_width(0).into(),
                },
                Space::with_width(8),
                album_grid::layout_toggle(grid_view),
            ]
            .align_y(Alignment::Center)
            .padding([12, 12]),
            body,
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// `AlbumGroup::artist` is empty when the server couldn't group by
/// AlbumArtist; `AlbumSelected` wants `None` in that case so it skips
/// multi-disc sibling merging.
fn artist_opt(group: &AlbumGroup) -> Option<String> {
    if group.artist.is_empty() {
        None
    } else {
        Some(group.artist.clone())
    }
}
