//! A text rendered as a clickable inline link (transparent button, accent on hover).

use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use iced::widget::{button, text};
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

/// Font used for the play/add glyphs. The bundled iced default font lacks
/// `▶`/`＋`, rendering them as tofu boxes; Segoe UI Symbol (always present on
/// Windows) has them.
const ICON_FONT: iced::Font = iced::Font::with_name("Segoe UI Symbol");

/// A compact icon/action button: no background at rest, hover reveals `BG_HOVER`
/// with accent text. Use for play (`▶`) and queue (`+`) row actions.
pub fn icon_btn<'a>(label: &'static str, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(15).font(ICON_FONT))
        .on_press(on_press)
        .padding([2, 8])
        .style(|_t: &iced::Theme, status: button::Status| {
            let (bg, text_color) = match status {
                button::Status::Hovered | button::Status::Pressed => (
                    Some(AppColors::BG_HOVER.into()),
                    AppColors::ACCENT,
                ),
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
        })
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
