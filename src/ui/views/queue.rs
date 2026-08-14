use crate::mpd::types::*;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::ui::widgets::link::{icon_btn_danger_maybe, icon_btn_tip, icon_btn_tip_maybe};
use crate::ui::widgets::song_row;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

/// Move up, move down, add-to-playlist, remove. All four are always rendered
/// (disabled where they don't apply), so this width is constant.
const ACTIONS_WIDTH: u16 = song_row::action_group_width(4);

pub fn view<'a>(
    queue: &'a [Song],
    current_pos: Option<u32>,
) -> Element<'a, Message> {
    let toolbar = row![
        text(format!("{} tracks", queue.len()))
            .size(13)
            .color(AppColors::TEXT_MUTED),
        Space::with_width(Length::Fill),
        button(text("Shuffle").size(12))
            .on_press(Message::QueueShuffle)
            .padding([4, 12]),
        button(text("Clear").size(12))
            .on_press(Message::QueueClear)
            .padding([4, 12]),
    ]
    .spacing(8)
    .padding([8, 12]);

    let header = container(
        row![
            // Marker cell + the row's 8px spacing + the number column, so the
            // header lines up with the rows. Right-aligned to sit over the
            // right-aligned numbers.
            text("#")
                .size(11)
                .width(song_row::MARKER_WIDTH + 8 + song_row::NUMBER_WIDTH)
                .align_x(iced::alignment::Horizontal::Right)
                .color(AppColors::TEXT_MUTED),
            text("Title").size(11).width(Length::FillPortion(3)).color(AppColors::TEXT_MUTED),
            text("Artist").size(11).width(Length::FillPortion(2)).color(AppColors::TEXT_MUTED),
            text("Album").size(11).width(Length::FillPortion(2)).color(AppColors::TEXT_MUTED),
            text("Time")
                .size(11)
                .width(song_row::DURATION_WIDTH)
                .align_x(iced::alignment::Horizontal::Right)
                .color(AppColors::TEXT_MUTED),
            // Reserves the action group's width. Without it the header's
            // FillPortion columns get ~134px more to share than the rows do,
            // and every label sits visibly right of the data under it.
            Space::with_width(ACTIONS_WIDTH),
        ]
        .spacing(8)
        .padding([4, 12]),
    )
    .style(|_theme: &iced::Theme| container::Style {
        border: iced::Border {
            width: 1.0,
            color: AppColors::BORDER,
            ..Default::default()
        },
        ..Default::default()
    });

    let mut items = column![].spacing(0);

    if queue.is_empty() {
        items = items.push(
            container(
                text("Queue is empty. Add songs from Artists, Albums, Browse, or Search.")
                    .size(14)
                    .color(AppColors::TEXT_MUTED),
            )
            .padding(20),
        );
    }

    for (i, song) in queue.iter().enumerate() {
        let pos = song.pos.unwrap_or(0);
        // The Queue is the one list that matches on position rather than URI —
        // a queue can hold the same file twice. See `widgets::song_row`.
        let is_current = song_row::is_current_pos(pos, current_pos);

        let bg = song_row::row_bg(i, is_current);
        let title_color = song_row::title_color(is_current);

        // Title plays the track; artist and album navigate to their views.
        let title_btn = button(
            text(song.display_title()).size(12).color(title_color),
        )
        .on_press(Message::QueuePlay(pos))
        .padding(0)
        .width(Length::FillPortion(3))
        .style(|_t: &iced::Theme, _s: button::Status| button::Style {
            background: None,
            text_color: AppColors::TEXT_PRIMARY,
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
        });

        let artist_btn = button(
            text(song.display_artist()).size(11),
        )
        .on_press(Message::ArtistSelected(song.display_artist().to_string()))
        .padding(0)
        .width(Length::FillPortion(2))
        .style(|_t: &iced::Theme, s: button::Status| button::Style {
            background: None,
            text_color: match s {
                button::Status::Hovered | button::Status::Pressed => AppColors::ACCENT,
                _ => AppColors::TEXT_SECONDARY,
            },
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
        });

        let album_btn = button(
            text(song.display_album()).size(11),
        )
        .on_press(Message::AlbumSelected(
            song.display_album().to_string(),
            Some(song.display_album_artist().to_string()),
        ))
        .padding(0)
        .width(Length::FillPortion(2))
        .style(|_t: &iced::Theme, s: button::Status| button::Style {
            background: None,
            text_color: match s {
                button::Status::Hovered | button::Status::Pressed => AppColors::ACCENT,
                _ => AppColors::TEXT_SECONDARY,
            },
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
        });

        // No "add to end of queue" here: these rows already *are* the queue.
        // See the row-action table in CLAUDE.md.
        //
        // Both arrows are always present, disabled at the ends rather than
        // omitted. Omitting one narrows the whole action group, which hands
        // the freed width back to the FillPortion columns to its left — so the
        // first and last rows' Title/Artist/Album/Time drifted right against
        // every row between them, and the last row's Move-up arrow rendered in
        // the Move-down column.
        let mut actions = row![].spacing(song_row::ACTION_SPACING);
        actions = actions.push(icon_btn_tip_maybe(
            icon::MOVE_UP,
            "Move up",
            (i > 0).then(|| Message::QueueMoveUp(pos)),
        ));
        actions = actions.push(icon_btn_tip_maybe(
            icon::MOVE_DOWN,
            "Move down",
            (i + 1 < queue.len()).then(|| Message::QueueMoveDown(pos)),
        ));
        actions = actions.push(icon_btn_tip(
            icon::ADD_PLAYLIST,
            "Add to playlist…",
            Message::OpenAddToPlaylist(vec![song.file.clone()]),
        ));
        // `id` is always set on a real queue response; disabled rather than
        // omitted for the same layout reason as the arrows above.
        actions = actions.push(icon_btn_danger_maybe(
            icon::REMOVE,
            "Remove from queue",
            song.id.map(Message::QueueRemove),
        ));

        // Fixed width so the columns to the left land in the same place on
        // every row, and so the header below can reserve the same amount.
        let actions = actions.width(ACTIONS_WIDTH);

        items = items.push(
            container(
                row![
                    song_row::playing_marker(is_current),
                    song_row::number(format!("{}", pos + 1), 12),
                    title_btn,
                    artist_btn,
                    album_btn,
                    song_row::duration(song.format_duration(), 11),
                    actions,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([5, 12])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                ..Default::default()
            }),
        );
    }

    container(
        column![
            toolbar,
            header,
            scrollable(items).height(Length::Fill),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
