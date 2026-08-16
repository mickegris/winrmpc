//! The standard page wrapper: full-size and vertically scrollable.
//!
//! Views that render a plain column of content — Settings, Outputs,
//! Partitions, Stats — had no scrollable at all, so anything past the bottom
//! of the window was simply **unreachable**. It went unnoticed because the
//! long views (the library lists) all scroll their *list*, and these ones are
//! only too tall on a short window. Settings is where it bit: with the
//! Appearance, Shortcuts and Storage sections added in 0.4.3 it outgrew a
//! small window and the lower half couldn't be reached.
//!
//! # Don't put a `Length::Fill` child inside this
//!
//! A vertical scrollable lays its content out with unbounded height, so a
//! child asking to fill that height has nothing sensible to resolve against.
//! Every view using [`page`] is a `Shrink`-height column; the outer container
//! is what fills the window.
//!
//! That restriction is also why Now Playing does **not** use this: its
//! two-column layout depends on `Fill` throughout (the lyrics pane fills the
//! height and scrolls internally). It is protected by `window::Settings`'
//! `min_size` instead.

use crate::ui::message::Message;
use iced::widget::{container, scrollable};
use iced::{Element, Length};

/// Wrap a view's content so it fills the window and scrolls when it doesn't
/// fit. The scrollbar only appears when the content actually overflows.
pub fn page<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        scrollable(content)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
