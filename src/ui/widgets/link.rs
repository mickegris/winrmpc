//! A text rendered as a clickable inline link (transparent button, accent on hover).

use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::mpd::types::SortKey;
use iced::widget::{button, container, pick_list, row, text, tooltip};
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
                button::Status::Hovered | button::Status::Pressed => AppColors::accent(),
                _ => AppColors::text_secondary(),
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
            button::Status::Hovered | button::Status::Pressed => AppColors::accent(),
            _ => AppColors::text_secondary(),
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

/// The Back button, shared by every view that has one.
///
/// This was hand-written **ten times**, each with its own copy of the style
/// block, so the back affordance could drift between views and twice nearly
/// did. One definition means it cannot.
pub fn back_button<'a>() -> Element<'a, Message> {
    button(
        row![
            icon::icon_sized(icon::BACK, 14),
            text("Back").size(14),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .on_press(Message::GoBack)
    .padding([4, 8])
    .style(|_t: &iced::Theme, status: button::Status| button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => AppColors::text_primary(),
            _ => AppColors::accent(),
        },
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
    })
    .into()
}

/// The tag values `display_artist()` / `display_album()` fall back to when a
/// song carries no such tag. Navigating to them produces a junk view — MPD has
/// no artist called "Unknown Artist" — so [`artist_link`] and [`album_link`]
/// render them as inert text instead of links.
// Re-exported from `mpd::types`, which produces them — a second definition
// here would be one rename away from a link that silently stops being gated.
pub use crate::mpd::types::{UNKNOWN_ALBUM, UNKNOWN_ARTIST};

/// Is this a real name, or the placeholder for a missing tag?
pub fn is_real_name(name: &str) -> bool {
    !name.is_empty() && name != UNKNOWN_ARTIST && name != UNKNOWN_ALBUM
}

/// The shared look for a name link: muted at rest, accent on hover.
///
/// Deliberately **not underlined** — at size 11-12 in a dense track list,
/// underlining every artist turns the list into a thicket.
fn name_link_style(_t: &iced::Theme, status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => AppColors::accent(),
            _ => AppColors::text_secondary(),
        },
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
    }
}

/// An artist name that navigates to that artist.
///
/// Use this **everywhere an artist name is rendered**, so no call site has to
/// remember to build `Message::ArtistSelected` or to check for the
/// missing-tag placeholder. Falls back to inert text when the name isn't real.
pub fn artist_link<'a>(name: &str, size: u16) -> Element<'a, Message> {
    if !is_real_name(name) {
        return text(name.to_string())
            .size(size)
            .color(AppColors::text_muted())
            .into();
    }
    button(text(name.to_string()).size(size))
        .on_press(Message::ArtistSelected(name.to_string()))
        .padding(0)
        .style(name_link_style)
        .into()
}

/// An album name that navigates to that album.
///
/// `artist` should be the **album artist** where it is known. Passing `None`
/// reaches `AlbumSelected`'s artist-less path, which skips multi-disc
/// expansion and degrades the bio lookup — see CLAUDE.md's "Album identity".
/// Prefer supplying it.
pub fn album_link<'a>(album: &str, artist: Option<&str>, size: u16) -> Element<'a, Message> {
    if !is_real_name(album) {
        return text(album.to_string())
            .size(size)
            .color(AppColors::text_muted())
            .into();
    }
    button(text(album.to_string()).size(size))
        .on_press(album_message(album, artist))
        .padding(0)
        .style(name_link_style)
        .into()
}

/// The `AlbumSelected` message for an album, with the artist normalised.
///
/// Split out of [`album_link`] so a *container* that navigates to an album —
/// the cover tile, a list row — builds the identical message rather than
/// assembling its own and forgetting to drop the missing-tag placeholder.
pub fn album_message(album: &str, artist: Option<&str>) -> Message {
    Message::AlbumSelected(
        album.to_string(),
        artist.filter(|a| is_real_name(a)).map(str::to_string),
    )
}

/// The A-Z / Z-A control for a name-only list (Artists, Genres, Playlists).
///
/// Deliberately text, not a glyph. The obvious reuse is barred anyway —
/// `icon::MOVE_UP`/`MOVE_DOWN` already *are* `arrow_upward`/`arrow_downward`,
/// and `icon::tests` asserts no two constants share a codepoint — but the
/// real reason is that an arrow doesn't say *what* is being sorted, while
/// "A-Z" does. Same argument as the player bar's Single and Consume keeping
/// their words.
pub fn sort_toggle<'a>(desc: bool) -> Element<'a, Message> {
    direction_button(SortKey::Name, desc)
}

/// The album lists' sort controls: what to order by, then which way.
///
/// The direction button's wording follows the key — "A-Z" is meaningless for
/// a date and "Oldest" is meaningless for a title — which is why it comes
/// from `SortKey::direction_label` rather than being fixed text.
pub fn album_sort_controls<'a>(key: SortKey, desc: bool) -> Element<'a, Message> {
    row![
        text("Sort").size(12).color(AppColors::text_muted()),
        pick_list(SortKey::ALL, Some(key), Message::SetSortKey).text_size(12),
        direction_button(key, desc),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .into()
}

fn direction_button<'a>(key: SortKey, desc: bool) -> Element<'a, Message> {
    with_tip(
        button(text(key.direction_label(desc)).size(12))
            .on_press(Message::ToggleSortDirection)
            .padding([4, 12])
            .into(),
        "Reverse the order",
    )
}

/// The shared look for a compact glyph button: no background at rest, hover
/// reveals `BG_HOVER` with accent text.
fn icon_btn_style(_t: &iced::Theme, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Hovered | button::Status::Pressed => {
            (Some(AppColors::bg_hover().into()), AppColors::accent())
        }
        // Disabled has to be visually distinct or the button lies: it looks
        // pressable and isn't. This arm used to fall into the catch-all.
        button::Status::Disabled => (None, AppColors::text_disabled()),
        _ => (None, AppColors::text_muted()),
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
    icon_btn_danger_maybe(glyph, tip, Some(on_press))
}

/// [`icon_btn_danger`] where the action may not apply. See
/// [`icon_btn_tip_maybe`] for why this disables rather than omits.
pub fn icon_btn_danger_maybe<'a>(
    glyph: &'static str,
    tip: &'static str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    with_tip(
        button(icon::icon(glyph))
            .on_press_maybe(on_press)
            .padding([2, 8])
            .style(|_t: &iced::Theme, status: button::Status| {
                let (bg, text_color) = match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        (Some(AppColors::bg_hover().into()), AppColors::error())
                    }
                    button::Status::Disabled => (None, AppColors::text_disabled()),
                    _ => (None, AppColors::error()),
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
    icon_btn_tip_maybe(glyph, tip, Some(on_press))
}

/// [`icon_btn_tip`] where the action may not apply to this row.
///
/// `None` renders the button **disabled rather than omitted**, and that is the
/// point: the row's action buttons are laid out after `FillPortion` columns,
/// so dropping one narrows the action group, hands the freed width back to the
/// fill columns, and shifts Title/Artist/Album/Time on that row alone. The
/// queue's first and last rows drifted visibly against every row between them,
/// and the last row's remaining arrow slid into the *other* arrow's column.
///
/// Same reasoning as `song_row::playing_marker`: a slot that changes width
/// with content moves everything beside it, so hold the slot and change what's
/// in it.
pub fn icon_btn_tip_maybe<'a>(
    glyph: &'static str,
    tip: &'static str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    with_tip(
        button(icon::icon(glyph))
            .on_press_maybe(on_press)
            .padding([2, 8])
            .style(icon_btn_style)
            .into(),
        tip,
    )
}

/// Wrap any element in the shared tooltip styling.
///
/// Public because the player bar's transport controls need it too, and a
/// second tooltip style would drift from this one.
pub fn with_tip<'a>(inner: Element<'a, Message>, tip: &'static str) -> Element<'a, Message> {
    tooltip(
        inner,
        container(text(tip).size(12))
            .padding([4, 8])
            .style(|_t: &iced::Theme| container::Style {
                background: Some(AppColors::bg_tertiary().into()),
                text_color: Some(AppColors::text_primary()),
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: AppColors::border(),
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
                button::Status::Hovered | button::Status::Pressed => AppColors::text_primary(),
                _ => AppColors::accent(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tag_placeholders_are_not_real_names() {
        // These are what `display_artist()`/`display_album()` return for an
        // untagged file. Linking them navigates to an artist MPD has never
        // heard of, which renders an empty page.
        assert!(!is_real_name(UNKNOWN_ARTIST));
        assert!(!is_real_name(UNKNOWN_ALBUM));
        assert!(!is_real_name(""));
        assert!(is_real_name("Tiamat"));
        assert!(is_real_name("AC/DC"));
    }

    #[test]
    fn album_message_carries_the_artist_when_it_is_real() {
        match album_message("Wildhoney", Some("Tiamat")) {
            Message::AlbumSelected(album, artist) => {
                assert_eq!(album, "Wildhoney");
                assert_eq!(artist.as_deref(), Some("Tiamat"));
            }
            other => panic!("expected AlbumSelected, got {other:?}"),
        }
    }

    /// The artist-less path skips multi-disc expansion and degrades the bio
    /// lookup, so it must only be reached when there is genuinely no artist —
    /// never because the placeholder was passed through as if it were one.
    #[test]
    fn album_message_drops_a_placeholder_artist() {
        for artist in [Some(UNKNOWN_ARTIST), Some(""), None] {
            match album_message("Greatest Hits", artist) {
                Message::AlbumSelected(_, resolved) => assert_eq!(
                    resolved, None,
                    "placeholder artist {artist:?} should not be sent as an artist"
                ),
                other => panic!("expected AlbumSelected, got {other:?}"),
            }
        }
    }
}
