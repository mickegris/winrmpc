//! Shared album-art grid, used by the Albums list, Recently Added and
//! Recently Played so all three look and behave the same.
//!
//! Every one of those views can be shown either as a compact text list or as
//! a grid of cover tiles (the Spotify/mikMPD shape); the choice is a single
//! app-wide flag, `AppConfig::album_grid_view`.

use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::ui::widgets::link;
use iced::widget::{button, column, container, image, row, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;

pub const TILE_SIZE: u16 = 120;

/// Cover edge length in list mode. Small enough that a long list still
/// scrolls comfortably, big enough to recognise a cover.
pub const LIST_THUMB: u16 = 36;

/// One cover tile: art (or a placeholder block), the album title, and the
/// artist beneath it. `caption` is an optional fourth line — "2 discs", "3d ago".
///
/// **The cover and the album title are one button; the artist is a separate
/// link.** Nesting the artist link *inside* an album-wide button would leave
/// one small region of a large clickable area doing something different, with
/// no way to tell by looking — so they are siblings instead. The cover plus
/// title keeps a big, forgiving target for the common action.
pub fn tile<'a>(
    art: Option<&'a iced::widget::image::Handle>,
    album: String,
    artist: String,
    caption: Option<String>,
) -> Element<'a, Message> {
    let art_widget: Element<'a, Message> = match art {
        Some(handle) => image(handle.clone())
            .width(TILE_SIZE)
            .height(TILE_SIZE)
            .into(),
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

    let cover_and_title = button(
        column![
            art_widget,
            Space::with_height(6),
            text(album.clone()).size(13).color(AppColors::TEXT_PRIMARY),
        ]
        .align_x(Alignment::Center)
        .width(TILE_SIZE),
    )
    .on_press(link::album_message(&album, Some(&artist)))
    .padding(0)
    .style(|_t: &iced::Theme, _s| button::Style {
        background: None,
        text_color: AppColors::TEXT_PRIMARY,
        border: iced::Border::default(),
        ..Default::default()
    });

    let mut col = column![cover_and_title, link::artist_link(&artist, 11)]
        .align_x(Alignment::Center)
        .width(TILE_SIZE);

    if let Some(c) = caption {
        col = col.push(text(c).size(10).color(AppColors::TEXT_MUTED));
    }

    container(col).padding(4).into()
}

/// The list-mode counterpart to a tile's cover: the same art at `LIST_THUMB`,
/// or the same placeholder block. Shared so that every view offering the
/// Grid/List switch shows art in *both* layouts — switching layout should
/// change the density, never which albums appear to have a cover.
pub fn list_thumb<'a>(
    art: Option<&'a iced::widget::image::Handle>,
) -> Element<'a, Message> {
    match art {
        Some(handle) => image(handle.clone())
            .width(LIST_THUMB)
            .height(LIST_THUMB)
            .into(),
        None => container(text("").size(1))
            .width(LIST_THUMB)
            .height(LIST_THUMB)
            .style(|_t: &iced::Theme| container::Style {
                background: Some(AppColors::BG_SECONDARY.into()),
                border: iced::Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
    }
}

/// Flows tiles into as many columns as the window is currently wide enough
/// for, via iced's wrapping row.
///
/// This used to chunk into a fixed 5 per row, which left a growing band of
/// dead space on the right of any window wider than 5 tiles (and clipped on
/// anything narrower). `Row::wrap` re-flows on every layout pass, so the
/// column count follows the window instead of a constant.
pub fn grid<'a>(tiles: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    row(tiles)
        .spacing(16)
        .width(Length::Fill)
        .wrap()
        .into()
}

/// The Grid/List switch. Shown in every view that supports both.
pub fn layout_toggle<'a>(grid_view: bool) -> Element<'a, Message> {
    // Icon and label are separate widgets because only the icon can use the
    // bundled icon font. The list glyph here is `icon::LIST`, distinct from the
    // playlist-add action's `icon::ADD_PLAYLIST` — both used to be `☰`.
    let (glyph, label) = if grid_view {
        (icon::LIST, "List")
    } else {
        (icon::GRID, "Grid")
    };
    button(
        row![icon::icon_sized(glyph, 14), text(label).size(12)]
            .spacing(6)
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::ToggleAlbumGridView)
    .padding([4, 12])
    .into()
}

/// Look up an album's cached art. Returns `None` when nothing has been
/// fetched for it yet, which renders as the placeholder block.
pub fn art_for<'a>(
    art_handles: &'a HashMap<String, iced::widget::image::Handle>,
    artist: &str,
    album: &str,
) -> Option<&'a iced::widget::image::Handle> {
    art_handles.get(&crate::mpd::types::art_key_for(artist, album))
}
