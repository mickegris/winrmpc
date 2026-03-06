use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, container, row, text, Column, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    artist_name: &'a str,
    albums: &'a [String],
    art_handles: &'a std::collections::HashMap<String, iced::widget::image::Handle>,
    bio: Option<&'a str>,
    show_bio: bool,
) -> Element<'a, Message> {
    let mut col = Column::new().spacing(0).padding(20);

    col = col.push(
        button(text("<- Back").size(14).color(AppColors::ACCENT))
            .on_press(Message::GoBack)
            .padding([4, 8]),
    );
    col = col.push(Space::with_height(12));

    // Artist header with art
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

    col = col.push(
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

    col = col.push(Space::with_height(12));

    // Wikipedia bio - expandable
    match bio {
        Some(bio_text) => {
            let toggle_label = if show_bio { "Hide info" } else { "Show info" };
            col = col.push(
                button(
                    text(toggle_label).size(12).color(AppColors::ACCENT),
                )
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
                col = col.push(Space::with_height(8));
                col = col.push(
                    container(
                        text(bio_text)
                            .size(12)
                            .color(AppColors::TEXT_SECONDARY),
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
        None => {
            col = col.push(
                text("Loading info...").size(11).color(AppColors::TEXT_MUTED),
            );
        }
    }

    col = col.push(Space::with_height(16));

    // Album list
    if albums.is_empty() {
        col = col.push(
            text("Loading albums...").size(14).color(AppColors::TEXT_MUTED),
        );
    }

    for (i, album) in albums.iter().enumerate() {
        let bg = if i % 2 == 0 {
            AppColors::ROW_EVEN
        } else {
            AppColors::ROW_ODD
        };

        let art_key = format!("{artist_name}-{album}");
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

        col = col.push(
            button(
                row![
                    art_widget,
                    Space::with_width(10),
                    text(album.as_str())
                        .size(14)
                        .color(AppColors::TEXT_PRIMARY),
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Message::AlbumSelected(album.clone()))
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

    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
