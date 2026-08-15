use crate::mpd::types::AlbumGroup;
use crate::ui::message::Message;
use crate::ui::widgets::link;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

/// Albums are `AlbumGroup`s rather than bare names so this view can navigate
/// **artist-scoped**, like every other album listing. It used to send
/// `AlbumSelected(name, None)`, and that artist-less path skips multi-disc
/// expansion — so opening a multi-disc album from a genre showed only the
/// tracks whose literal `Album` tag matched the collapsed base name.
pub fn view<'a>(
    genre_name: &'a str,
    albums: &'a [AlbumGroup],
) -> Element<'a, Message> {
    let mut list = column![].spacing(0);
    for (i, album) in albums.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        let mut label = row![link::album_link(&album.base, Some(&album.artist), 14)]
            .spacing(8)
            .align_y(Alignment::Center);
        if !album.artist.is_empty() {
            label = label.push(link::artist_link(&album.artist, 12));
        }
        if album.variants.len() > 1 {
            label = label.push(
                text(format!("{} discs", album.variants.len()))
                    .size(11)
                    .color(AppColors::ACCENT),
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

    container(
        column![
            row![
                button(text("<- Back").size(14).color(AppColors::ACCENT))
                    .on_press(Message::GoBack)
                    .padding([4, 8]),
                Space::with_width(12),
                text(genre_name).size(24).color(AppColors::TEXT_PRIMARY),
                Space::with_width(12),
                text(format!("{} albums", albums.len()))
                    .size(14)
                    .color(AppColors::TEXT_MUTED),
            ]
            .align_y(Alignment::Center)
            .padding([12, 12]),
            scrollable(list).height(Length::Fill),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
