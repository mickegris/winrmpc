use crate::mpd::types::*;
use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::ui::widgets::link;
use iced::widget::{button, column, container, pick_list, row, slider, text, Space};
use iced::{Alignment, Element, Length};

/// Transport button geometry. Kept as constants because the relationship
/// between them is what decides whether the glyph renders at all — see
/// `styled_control_btn` and `transport_glyph_fits_its_button`.
const TRANSPORT_ICON: u16 = 20;
const TRANSPORT_H: u16 = 36;
const TRANSPORT_W: u16 = 56;
const TRANSPORT_PAD_Y: u16 = 3;
/// Slack the button must keep over the glyph's line box.
///
/// Not zero: iced rounds line boxes, and a design that only *just* fits is one
/// rounding away from the blank-button bug all over again.
const TRANSPORT_MIN_SLACK: f32 = 3.0;
/// iced's default line height is 1.3x the text size.
const LINE_HEIGHT_FACTOR: f32 = 1.3;

/// Width of the bottom-left song-info slot. The clip container and the
/// column inside it must agree on this or the truncation lands in the wrong
/// place.
///
/// Bounded on both sides below, at compile time rather than in a test: below
/// ~200px two 12px links plus a separator can't show a plausible
/// "Artist – Album" at all, and above ~320 the slot crowds the transport
/// controls at `min_size`'s 1000px width.
const SONG_INFO_WIDTH: u16 = 250;
const _: () = assert!(SONG_INFO_WIDTH >= 200 && SONG_INFO_WIDTH <= 320);

/// MPD's four replay-gain modes (protocol: `replay_gain_mode {MODE}`).
const REPLAY_GAIN_MODES: [&str; 4] = ["off", "track", "album", "auto"];

pub fn view<'a>(
    status: &'a Status,
    current_song: &'a Option<Song>,
    replay_gain_mode: Option<&'a str>,
) -> Element<'a, Message> {
    let song_info: Element<'a, Message> = match current_song {
        Some(song) => column![
            text(song.display_title())
                .size(14)
                .color(AppColors::text_primary()),
            // The separator is its own widget because a link is a `button`
            // and can't share a `text` with the name beside it — the same
            // reason icon and label are always separate widgets.
            //
            // `album_link` gets the *album* artist, matching
            // `now_playing.rs`, so it opens the same page the cover tile
            // does. Both helpers degrade the missing-tag placeholders to
            // inert text on their own.
            row![
                link::artist_link(song.display_artist(), 12),
                text(" – ").size(12).color(AppColors::text_muted()),
                link::album_link(
                    song.display_album(),
                    Some(song.display_album_artist()),
                    12,
                ),
            ]
            .align_y(Alignment::Center),
        ]
        .width(SONG_INFO_WIDTH)
        .into(),
        None => text("No song playing")
            .size(14)
            .color(AppColors::text_muted())
            .width(SONG_INFO_WIDTH)
            .into(),
    };

    // Clipped, and that is load-bearing. As one `text` the pair word-wrapped
    // to a second line, which silently grew the whole bar (it has no fixed
    // height) whenever a track had long names. Three siblings in a `row`
    // can't wrap — a row overflows, and iced widgets don't clip to their
    // parent, so a long pair would draw straight over the Previous button.
    // Clipping truncates at the slot edge instead and keeps the bar's height
    // constant across track changes.
    let song_info = container(song_info).width(SONG_INFO_WIDTH).clip(true);

    let elapsed = status.elapsed.map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let duration = status
        .duration
        .map(|d| d.as_secs_f64())
        .unwrap_or(1.0)
        .max(0.01);

    let is_playing = status.state == PlayState::Play;

    let controls = row![
        styled_control_btn(icon::PREV, "Previous track", Message::Previous, false),
        if is_playing {
            styled_control_btn(icon::PAUSE, "Pause", Message::Pause, true)
        } else {
            styled_control_btn(icon::PLAY, "Play", Message::Play, true)
        },
        // "Stop" earns a tooltip that pause doesn't: in MPD it resets the
        // playback position, which the glyph alone doesn't say.
        styled_control_btn(icon::STOP, "Stop (resets position)", Message::Stop, false),
        styled_control_btn(icon::NEXT, "Next track", Message::Next, false),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let progress = row![
        text(format_time(elapsed))
            .size(12)
            .color(AppColors::text_muted()),
        slider(0.0..=duration, elapsed, Message::SeekTo)
            .width(Length::Fill)
            .step(0.5),
        text(format_time(duration))
            .size(12)
            .color(AppColors::text_muted()),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let volume_slider = row![
        icon::icon_sized(icon::VOLUME, 15).color(AppColors::text_muted()),
        slider(
            0.0..=100.0,
            status.volume as f64,
            Message::VolumeChanged
        )
        .width(100)
        .step(1.0),
        text(format!("{}%", status.volume))
            .size(12)
            .color(AppColors::text_muted()),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    // No "On"/"Off" suffix: `mode_btn` already carries state in its accent
    // background, so the words were saying twice what the colour says once.
    // The `1x` forms stay, because oneshot is the one state a colour cannot
    // express.
    //
    // Repeat shows `repeat_one` when single is also on — that pairing is what
    // "repeat this track" actually means, and showing it costs nothing now
    // that both glyphs are bundled.
    let repeat_glyph = if status.single != SingleState::Off {
        icon::REPEAT_ONE
    } else {
        icon::REPEAT
    };
    let single_text = match status.single {
        SingleState::Oneshot => "Single 1x",
        _ => "Single",
    };
    let consume_text = match status.consume {
        ConsumeState::Oneshot => "Consume 1x",
        _ => "Consume",
    };

    // Crossfade and replay gain are server-wide playback settings exactly
    // like repeat/random/single/consume, so they sit beside them rather than
    // in one view's header — stacked in their own column to the *left* of
    // the mode buttons, crossfade above replay gain.
    let crossfade_secs = status.crossfade.unwrap_or(0);
    let crossfade_control = row![
        text("Crossfade").size(12).color(AppColors::text_secondary()),
        Space::with_width(Length::Fill),
        small_icon_btn(icon::MINUS, Message::SetCrossfade(crossfade_secs.saturating_sub(1))),
        text(format!("{crossfade_secs}s"))
            .size(13)
            .color(AppColors::text_primary()),
        small_icon_btn(icon::ADD_QUEUE, Message::SetCrossfade(crossfade_secs + 1)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    // Labelled: a bare dropdown reading "off"/"track"/"album"/"auto" gives
    // no clue what it controls.
    let replay_gain_control = row![
        text("Replay Gain").size(12).color(AppColors::text_secondary()),
        Space::with_width(Length::Fill),
        pick_list(
            REPLAY_GAIN_MODES.to_vec(),
            replay_gain_mode,
            |m: &str| Message::SetReplayGainMode(m.to_string()),
        )
        .text_size(12)
        .padding([3, 8]),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let audio_settings = column![crossfade_control, replay_gain_control]
        .spacing(5)
        .width(190);

    let mode_indicators = row![
        audio_settings,
        Space::with_width(14),
        column![
            row![
                mode_btn(repeat_glyph, "Repeat", status.repeat, Message::ToggleRepeat),
                mode_btn(icon::SHUFFLE, "Random", status.random, Message::ToggleRandom),
            ]
            .spacing(4),
            row![
                mode_btn(
                    icon::REPEAT_ONE,
                    single_text,
                    status.single != SingleState::Off,
                    Message::ToggleSingle
                ),
                mode_btn(
                    icon::REMOVE,
                    consume_text,
                    status.consume != ConsumeState::Off,
                    Message::ToggleConsume
                ),
            ]
            .spacing(4),
        ]
        .spacing(4),
    ]
    .align_y(Alignment::Center);

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
        background: Some(AppColors::bg_secondary().into()),
        border: iced::Border {
            width: 1.0,
            color: AppColors::border(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// A transport control: an icon-font glyph on the shared 52x28 button, with a
/// tooltip. The glyphs (⏮ ▶/⏸ ⏹ ⏭) are the one genuinely universal icon
/// vocabulary in a music player — nobody needs "Prev" spelled out — but the
/// tooltip is what makes them honest for anyone who does.
fn styled_control_btn<'a>(
    glyph: &'static str,
    tip: &'static str,
    msg: Message,
    primary: bool,
) -> Element<'a, Message> {
    let bg = if primary {
        AppColors::accent()
    } else {
        AppColors::bg_tertiary()
    };
    let fg = if primary {
        AppColors::bg_primary()
    } else {
        AppColors::text_primary()
    };

    link::with_tip(
        button(
            container(icon::icon_sized(glyph, TRANSPORT_ICON).color(fg))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .on_press(msg)
        .width(TRANSPORT_W)
        .height(TRANSPORT_H)
        // **Explicit padding is load-bearing.** With `button`'s default
        // padding of 5 the content box was 28 - 10 = 18px tall, less than the
        // ~22px line box a 17px glyph needs, and the glyph did not render at
        // all — blank buttons, not clipped ones. The mode buttons never showed
        // this because they set their own smaller padding.
        //
        // The invariant: TRANSPORT_H - 2*vertical padding must exceed
        // TRANSPORT_ICON * iced's 1.3 default line height. A test pins it.
        .padding([TRANSPORT_PAD_Y, 8])
        .style(move |_theme: &iced::Theme, _status| button::Style {
            background: Some(bg.into()),
            text_color: fg,
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into(),
        tip,
    )
}

/// A playback-mode toggle: glyph **plus** the word.
///
/// The words stay because *Single* and *Consume* are MPD concepts with no
/// standard icon — "remove each track from the queue after playing it" is not
/// something a glyph conveys to anyone who doesn't already know MPD. And they
/// are tri-state (`Off`/`On`/`Oneshot`), which a colour alone can't show.
fn mode_btn<'a>(
    glyph: &'static str,
    label: &'a str,
    active: bool,
    msg: Message,
) -> Element<'a, Message> {
    let bg = if active {
        AppColors::accent()
    } else {
        AppColors::bg_tertiary()
    };
    let fg = if active {
        AppColors::bg_primary()
    } else {
        AppColors::text_muted()
    };

    button(
        container(
            row![
                icon::icon_sized(glyph, 13).color(fg),
                text(label.to_string()).size(11).color(fg),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill),
    )
    .on_press(msg)
    .height(24)
    .width(88)
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

/// Compact square button for the crossfade −/+ steppers.
/// [`small_btn`] with an icon-font glyph instead of a text label. The `+`/`−`
/// pair has to go through the icon font together: `−` (U+2212) is not in every
/// system font, and a matched pair drawn from two different fonts looks it.
fn small_icon_btn<'a>(glyph: &'static str, msg: Message) -> Element<'a, Message> {
    button(icon::icon_sized(glyph, 13).color(AppColors::text_primary()))
        .padding([2, 8])
        .on_press(msg)
        .style(|_t: &iced::Theme, _s| button::Style {
            background: Some(AppColors::bg_secondary().into()),
            text_color: AppColors::text_secondary(),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn small_btn(label: &str, msg: Message) -> Element<'_, Message> {
    button(text(label).size(13).color(AppColors::text_primary()))
        .padding([2, 8])
        .on_press(msg)
        .style(|_t: &iced::Theme, _s| button::Style {
            background: Some(AppColors::bg_secondary().into()),
            text_color: AppColors::text_secondary(),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glyph should also *look* like the button's main content rather than a
    /// speck in a wide box — 16px in a 52x28 button read as mostly air.
    #[test]
    fn the_transport_glyph_fills_a_reasonable_share_of_its_button() {
        let ratio = f32::from(TRANSPORT_ICON) / f32::from(TRANSPORT_H);
        assert!(
            (0.5..=0.72).contains(&ratio),
            "glyph/button height ratio {ratio:.2} — below ~0.5 it reads as a \
             speck, above ~0.72 it crowds the edges"
        );
    }

    /// The transport glyphs rendered as **nothing** on first real use: the
    /// button's default padding of 5 left an 18px content box for a glyph
    /// whose line box needs ~22px, and iced drew no line at all rather than a
    /// clipped one. Blank, not tofu — which is why it looked like a font
    /// problem and wasn't.
    #[test]
    fn transport_glyph_fits_its_button() {
        let content_h = f32::from(TRANSPORT_H - 2 * TRANSPORT_PAD_Y);
        let line_h = f32::from(TRANSPORT_ICON) * LINE_HEIGHT_FACTOR;
        assert!(
            content_h >= line_h + TRANSPORT_MIN_SLACK,
            "a {TRANSPORT_ICON}px glyph needs {line_h:.1}px of line box (plus \
             {TRANSPORT_MIN_SLACK}px slack) but the button only offers \
             {content_h:.1}px — it will render blank"
        );
    }
}
