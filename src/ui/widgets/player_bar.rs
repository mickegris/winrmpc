use crate::mpd::types::*;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, column, container, row, slider, text, Space};
use iced::{Alignment, Element, Length};

pub fn view<'a>(
    status: &'a Status,
    current_song: &'a Option<Song>,
) -> Element<'a, Message> {
    let song_info: Element<'a, Message> = match current_song {
        Some(song) => column![
            text(song.display_title())
                .size(14)
                .color(AppColors::TEXT_PRIMARY),
            text(format!(
                "{} - {}",
                song.display_artist(),
                song.display_album()
            ))
            .size(12)
            .color(AppColors::TEXT_SECONDARY),
        ]
        .width(250)
        .into(),
        None => text("No song playing")
            .size(14)
            .color(AppColors::TEXT_MUTED)
            .width(250)
            .into(),
    };

    let elapsed = status.elapsed.map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let duration = status
        .duration
        .map(|d| d.as_secs_f64())
        .unwrap_or(1.0)
        .max(0.01);

    let is_playing = status.state == PlayState::Play;

    let play_pause = styled_control_btn(
        if is_playing { "Pause" } else { "Play" },
        if is_playing { Message::Pause } else { Message::Play },
        true,
    );

    let controls = row![
        styled_control_btn("Prev", Message::Previous, false),
        play_pause,
        styled_control_btn("Next", Message::Next, false),
        styled_control_btn("Stop", Message::Stop, false),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let progress = row![
        text(format_time(elapsed))
            .size(12)
            .color(AppColors::TEXT_MUTED),
        slider(0.0..=duration, elapsed, Message::SeekTo)
            .width(Length::Fill)
            .step(0.5),
        text(format_time(duration))
            .size(12)
            .color(AppColors::TEXT_MUTED),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let volume_slider = row![
        text("Vol").size(12).color(AppColors::TEXT_MUTED),
        slider(
            0.0..=100.0,
            status.volume as f64,
            Message::VolumeChanged
        )
        .width(100)
        .step(1.0),
        text(format!("{}%", status.volume))
            .size(12)
            .color(AppColors::TEXT_MUTED),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let repeat_label = if status.repeat { "Repeat: On" } else { "Repeat: Off" };
    let random_label = if status.random { "Random: On" } else { "Random: Off" };
    let single_label = match status.single {
        SingleState::On => "Single: On",
        SingleState::Oneshot => "Single: 1x",
        SingleState::Off => "Single: Off",
    };
    let consume_label = match status.consume {
        ConsumeState::On => "Consume: On",
        ConsumeState::Oneshot => "Consume: 1x",
        ConsumeState::Off => "Consume: Off",
    };

    let mode_indicators = row![
        mode_btn(repeat_label, status.repeat, Message::ToggleRepeat),
        mode_btn(random_label, status.random, Message::ToggleRandom),
        mode_btn(single_label, status.single != SingleState::Off, Message::ToggleSingle),
        mode_btn(consume_label, status.consume != ConsumeState::Off, Message::ToggleConsume),
    ]
    .spacing(2);

    container(
        column![
            progress,
            row![
                song_info,
                Space::with_width(Length::Fill),
                controls,
                Space::with_width(Length::Fill),
                mode_indicators,
                Space::with_width(16),
                volume_slider,
            ]
            .align_y(Alignment::Center)
            .spacing(12),
        ]
        .spacing(6)
        .padding([8, 16]),
    )
    .width(Length::Fill)
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(AppColors::BG_SECONDARY.into()),
        border: iced::Border {
            width: 1.0,
            color: AppColors::BORDER,
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn styled_control_btn(label: &str, msg: Message, primary: bool) -> Element<'_, Message> {
    let bg = if primary {
        AppColors::ACCENT
    } else {
        AppColors::BG_TERTIARY
    };
    let fg = if primary {
        AppColors::BG_PRIMARY
    } else {
        AppColors::TEXT_PRIMARY
    };

    button(
        container(
            text(label.to_string())
                .size(12)
                .color(fg),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill),
    )
    .on_press(msg)
    .width(52)
    .height(28)
    .style(move |_theme: &iced::Theme, _status| button::Style {
        background: Some(bg.into()),
        text_color: fg,
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn mode_btn(label: &str, active: bool, msg: Message) -> Element<'_, Message> {
    let bg = if active {
        AppColors::ACCENT
    } else {
        AppColors::BG_TERTIARY
    };
    let fg = if active {
        AppColors::BG_PRIMARY
    } else {
        AppColors::TEXT_MUTED
    };

    button(
        container(
            text(label.to_string()).size(10).color(fg),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill),
    )
    .on_press(msg)
    .height(22)
    .padding([2, 6])
    .style(move |_theme: &iced::Theme, _status| button::Style {
        background: Some(bg.into()),
        text_color: fg,
        border: iced::Border {
            radius: 3.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn format_time(secs: f64) -> String {
    let total = secs as u64;
    let m = total / 60;
    let s = total % 60;
    format!("{m}:{s:02}")
}
