use crate::mpd::types::{art_key_for, AlbumGroup, Song};
use crate::ui::widgets::link;
use crate::ui::widgets::song_row;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, container, row, text, Column, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    artist_name: &'a str,
    albums: &'a [AlbumGroup],
    art_handles: &'a std::collections::HashMap<String, iced::widget::image::Handle>,
    bio: Option<&'a str>,
    show_bio: bool,
    current_song: Option<&'a Song>,
) -> Element<'a, Message> {
    // Fixed header
    let mut header = Column::new().spacing(4).padding(20);

    header = header.push(
        link::back_button(),
    );
    header = header.push(Space::with_height(8));

    let artist_art_key = format!("artist:{artist_name}");
    let artist_art: Element<'a, Message> =
        if let Some(handle) = art_handles.get(&artist_art_key) {
            iced::widget::image(handle.clone())
                .width(120)
                .height(120)
                .into()
        } else {
            container(text("").size(1))
                .width(120)
                .height(120)
                .style(|_theme: &iced::Theme| container::Style {
                    background: Some(AppColors::BG_PRIMARY.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

    header = header.push(
        row![
            artist_art,
            Space::with_width(16),
            Column::new()
                .push(text(artist_name).size(26).color(AppColors::TEXT_PRIMARY))
                .push(Space::with_height(4))
                .push(
                    text(format!("{} albums", albums.len()))
                        .size(14)
                        .color(AppColors::TEXT_MUTED)
                ),
        ]
        .align_y(Alignment::Center),
    );

    header = header.push(Space::with_height(8));

    match bio {
        Some(bio_text) => {
            let toggle_label = if show_bio { "Hide info" } else { "Show info" };
            header = header.push(
                button(text(toggle_label).size(12).color(AppColors::ACCENT))
                    .on_press(Message::ToggleArtistBio)
                    .padding([4, 8])
                    .style(|_theme: &iced::Theme, _status| button::Style {
                        background: None,
                        text_color: AppColors::ACCENT,
                        border: iced::Border::default(),
                        ..Default::default()
                    }),
            );
            if show_bio {
                header = header.push(Space::with_height(4));
                header = header.push(
                    container(
                        text(bio_text).size(12).color(AppColors::TEXT_SECONDARY),
                    )
                    .padding(12)
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(AppColors::BG_SECONDARY.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            color: AppColors::BORDER,
                            width: 1.0,
                        },
                        ..Default::default()
                    }),
                );
            }
        }
        None => {}
    }

    header = header.push(Space::with_height(8));

    // Scrollable album list
    let mut album_list = Column::new().spacing(0);

    if albums.is_empty() {
        album_list = album_list.push(
            container(
                text("Loading albums...").size(14).color(AppColors::TEXT_MUTED),
            )
            .padding([10, 20]),
        );
    }

    for (i, group) in albums.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        // group.base is already disc-stripped; art_key_for is a no-op
        // re-strip here, kept for consistency with every other art-cache
        // key site (all go through the same helper so they can't drift).
        let art_key = art_key_for(artist_name, &group.base);
        let art_widget: Element<'a, Message> =
            if let Some(handle) = art_handles.get(&art_key) {
                iced::widget::image(handle.clone())
                    .width(40)
                    .height(40)
                    .into()
            } else {
                container(text("").size(1))
                    .width(40)
                    .height(40)
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(AppColors::BG_PRIMARY.into()),
                        border: iced::Border {
                            radius: 3.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
            };

        let mut label = row![
            art_widget,
            Space::with_width(10),
            text(group.base.as_str())
                .size(14)
                .color(song_row::title_color(song_row::is_current_album(
                    artist_name,
                    &group.base,
                    current_song,
                ))),
        ]
        .align_y(Alignment::Center);

        if group.variants.len() > 1 {
            label = label.push(Space::with_width(8));
            label = label.push(
                text(format!("{} discs", group.variants.len()))
                    .size(11)
                    .color(AppColors::ACCENT),
            );
        }

        album_list = album_list.push(
            button(label)
                .on_press(Message::AlbumSelected(
                    group.base.clone(),
                    Some(artist_name.to_string()),
                ))
                .padding([4, 10])
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme, _status| button::Style {
                    background: Some(bg.into()),
                    text_color: AppColors::TEXT_PRIMARY,
                    border: iced::Border::default(),
                    ..Default::default()
                }),
        );
    }

    iced::widget::column![
        header,
        iced::widget::scrollable(
            container(album_list).padding([0, 20])
        )
        .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
