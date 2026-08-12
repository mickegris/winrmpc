use crate::mpd::types::AlbumGroup;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::album_grid;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;

pub fn view<'a>(
    albums: &'a [AlbumGroup],
    title: &'a str,
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
    grid_view: bool,
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
                    Message::AlbumSelected(group.base.clone(), artist_opt(group)),
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
                AppColors::ROW_EVEN
            } else {
                AppColors::ROW_ODD
            };

            // Same cover as the grid, just small — so switching layouts
            // doesn't change which albums appear to have art.
            let thumb = album_grid::list_thumb(album_grid::art_for(
                art_handles,
                &group.artist,
                &group.base,
            ));

            let mut label = row![
                thumb,
                text(group.base.as_str()).size(14).color(AppColors::TEXT_PRIMARY),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            if !group.artist.is_empty() {
                label = label.push(
                    text(group.artist.as_str()).size(12).color(AppColors::TEXT_MUTED),
                );
            }
            if group.variants.len() > 1 {
                label = label.push(
                    text(format!("{} discs", group.variants.len()))
                        .size(11)
                        .color(AppColors::ACCENT),
                );
            }

            list = list.push(
                button(label)
                    .on_press(Message::AlbumSelected(group.base.clone(), artist_opt(group)))
                    .padding([7, 12])
                    .width(Length::Fill)
                    .style(move |_theme: &iced::Theme, _status| button::Style {
                        background: Some(bg.into()),
                        text_color: AppColors::TEXT_PRIMARY,
                        border: iced::Border::default(),
                        ..Default::default()
                    }),
            );
        }
        scrollable(list).height(Length::Fill).into()
    };

    container(
        column![
            row![
                button(text("<- Back").size(14).color(AppColors::ACCENT))
                    .on_press(Message::GoBack)
                    .padding([4, 8]),
                Space::with_width(12),
                text(title).size(24).color(AppColors::TEXT_PRIMARY),
                Space::with_width(12),
                text(format!("{} albums", albums.len()))
                    .size(14)
                    .color(AppColors::TEXT_MUTED),
                Space::with_width(Length::Fill),
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
