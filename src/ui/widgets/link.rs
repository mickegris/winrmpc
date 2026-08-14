//! A text rendered as a clickable inline link (transparent button, accent on hover).

use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use iced::widget::{button, container, row, text, tooltip};
use iced::Element;

/// Build a link-styled button: no background, secondary text that brightens to
/// the accent colour on hover. Emits `on_press` when clicked.
pub fn link<'a>(
    label: impl text::IntoFragment<'a>,
    size: u16,
    on_press: Message,
) -> Element<'a, Message> {
    button(text(label).size(size))
        .on_press(on_press)
        .padding(0)
        .style(|_t: &iced::Theme, status: button::Status| {
            let text_color = match status {
                button::Status::Hovered | button::Status::Pressed => AppColors::ACCENT,
                _ => AppColors::TEXT_SECONDARY,
            };
            button::Style {
                background: None,
                text_color,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            }
        })
        .into()
}

/// [`link`] with a leading icon.
///
/// The icon and the label are separate widgets on purpose: they need different
/// fonts, so `"\u{1F551} History"` as a single string could only ever render
/// one of the two correctly.
pub fn link_icon<'a>(
    glyph: &'static str,
    label: impl text::IntoFragment<'a>,
    size: u16,
    on_press: Message,
) -> Element<'a, Message> {
    button(
        row![icon::icon_sized(glyph, size + 2), text(label).size(size)]
            .spacing(5)
            .align_y(iced::Alignment::Center),
    )
    .on_press(on_press)
    .padding(0)
    .style(|_t: &iced::Theme, status: button::Status| {
        let text_color = match status {
            button::Status::Hovered | button::Status::Pressed => AppColors::ACCENT,
            _ => AppColors::TEXT_SECONDARY,
        };
        button::Style {
            background: None,
            text_color,
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
        }
    })
    .into()
}

/// The shared look for a compact glyph button: no background at rest, hover
/// reveals `BG_HOVER` with accent text.
fn icon_btn_style(_t: &iced::Theme, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Hovered | button::Status::Pressed => {
            (Some(AppColors::BG_HOVER.into()), AppColors::ACCENT)
        }
        _ => (None, AppColors::TEXT_MUTED),
    };
    button::Style {
        background: bg,
        text_color,
        border: iced::Border {
            radius: 3.0.into(),
            ..Default::default()
        },
        shadow: iced::Shadow::default(),
    }
}

/// A compact icon/action button. `glyph` must be a constant from
/// [`crate::ui::widgets::icon`] — that module owns the bundled font, and a
/// glyph from anywhere else is not guaranteed to render off Windows.
///
/// Prefer [`icon_btn_tip`]: a bare glyph with no label and no tooltip is the
/// other half of why these buttons were hard to read.
pub fn icon_btn<'a>(glyph: &'static str, on_press: Message) -> Element<'a, Message> {
    button(icon::icon(glyph))
        .on_press(on_press)
        .padding([2, 8])
        .style(icon_btn_style)
        .into()
}

/// A tooltipped icon button that reads as destructive — the resting glyph is
/// `ERROR`-coloured rather than muted. For remove/delete actions only.
pub fn icon_btn_danger<'a>(
    glyph: &'static str,
    tip: &'static str,
    on_press: Message,
) -> Element<'a, Message> {
    with_tip(
        button(icon::icon(glyph))
            .on_press(on_press)
            .padding([2, 8])
            .style(|_t: &iced::Theme, status: button::Status| {
                let (bg, text_color) = match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        (Some(AppColors::BG_HOVER.into()), AppColors::ERROR)
                    }
                    _ => (None, AppColors::ERROR),
                };
                button::Style {
                    background: bg,
                    text_color,
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    shadow: iced::Shadow::default(),
                }
            })
            .into(),
        tip,
    )
}

/// [`icon_btn`] with a hover tooltip naming the action.
///
/// Row actions are unlabelled by necessity — four of them per row leaves no
/// width for text — so the tooltip is the only thing that answers "what does
/// this do" before the click. Wording is fixed per action in the table in
/// CLAUDE.md so the same button never reads differently between views.
pub fn icon_btn_tip<'a>(
    glyph: &'static str,
    tip: &'static str,
    on_press: Message,
) -> Element<'a, Message> {
    with_tip(icon_btn(glyph, on_press), tip)
}

/// Wrap any element in the shared tooltip styling.
fn with_tip<'a>(inner: Element<'a, Message>, tip: &'static str) -> Element<'a, Message> {
    tooltip(
        inner,
        container(text(tip).size(12))
            .padding([4, 8])
            .style(|_t: &iced::Theme| container::Style {
                background: Some(AppColors::BG_TERTIARY.into()),
                text_color: Some(AppColors::TEXT_PRIMARY),
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: AppColors::BORDER,
                },
                ..Default::default()
            }),
        // Above the row: the right-most actions sit near the window edge, where
        // a tooltip placed to the side would clip.
        tooltip::Position::Top,
    )
    .into()
}

/// Like [`link`] but starts in the accent colour (for prominent links such as
/// the artist line on the Now Playing view).
pub fn link_accent<'a>(
    label: impl text::IntoFragment<'a>,
    size: u16,
    on_press: Message,
) -> Element<'a, Message> {
    button(text(label).size(size))
        .on_press(on_press)
        .padding(0)
        .style(|_t: &iced::Theme, status: button::Status| {
            let text_color = match status {
                button::Status::Hovered | button::Status::Pressed => AppColors::TEXT_PRIMARY,
                _ => AppColors::ACCENT,
            };
            button::Style {
                background: None,
                text_color,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            }
        })
        .into()
}
