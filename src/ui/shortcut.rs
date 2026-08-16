//! Keyboard shortcuts.
//!
//! # Why the mapping is a pure function
//!
//! `iced::keyboard::on_key_press` takes a **`fn` pointer**, not a closure
//! (`iced_futures-0.13.2/src/keyboard.rs:13`), so the subscription cannot
//! capture any app state. It therefore emits a raw
//! [`Message::KeyPressed`](crate::ui::message::Message::KeyPressed) and the
//! decision happens here, in `update`, where the state is.
//!
//! That turns out to be the better shape anyway: [`resolve`] is a pure
//! function over an explicit [`Context`], so the thing most likely to go wrong
//! — a shortcut firing while someone is typing — is directly testable without
//! a window.
//!
//! # Why the gate is per-view
//!
//! iced 0.13's `text_input` has **no** focus/blur callbacks, and
//! `operation::focusable::find_focused()` resolves through a `Task`, so it is
//! asynchronous and cannot gate a keypress being handled right now. There is
//! no way to ask "is a text box focused?" synchronously.
//!
//! So bare keys are suppressed on the views that *contain* a text input, and
//! modifier-based shortcuts work everywhere. Crude, but it never eats a
//! keystroke someone meant as text — which is the only failure that really
//! matters here.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

use crate::ui::message::{Message, View};

/// Seconds moved by one arrow-key seek.
pub const SEEK_STEP: f64 = 5.0;
/// Volume points moved by one Ctrl+arrow.
pub const VOLUME_STEP: f64 = 5.0;

/// What [`resolve`] needs to know about the app.
///
/// Passed explicitly rather than read from `App` so the mapping stays a pure
/// function that tests can drive.
#[derive(Debug, Clone)]
pub struct Context {
    pub view: View,
    pub is_playing: bool,
    /// Seconds into the current track.
    pub elapsed: f64,
    /// Track length in seconds, for clamping a seek.
    pub duration: f64,
    pub volume: i32,
}

/// Does this view hold a `text_input`?
///
/// Verified by grepping for `text_input(` — Search, Settings, Radio, CD,
/// Partitions, Playlists and Add-to-Playlist. On these, bare keys are text.
///
/// `PlaylistDetail` is **not** in the list: the rename field lives in the
/// playlists *list*, not the detail view.
pub fn view_has_text_input(view: &View) -> bool {
    matches!(
        view,
        View::Search
            | View::Settings
            | View::Radio
            | View::CD
            | View::Partitions
            | View::Playlists
            | View::AddToPlaylist
    )
}

/// Map a keypress to a message, or `None` to ignore it.
pub fn resolve(key: &Key, mods: Modifiers, ctx: &Context) -> Option<Message> {
    let bare_ok = !view_has_text_input(&ctx.view);
    let ctrl = mods.command();

    match key {
        // --- always safe: modified ---
        Key::Named(Named::ArrowRight) if ctrl => Some(Message::Next),
        Key::Named(Named::ArrowLeft) if ctrl => Some(Message::Previous),
        // Ctrl rather than bare Up/Down: in a scrollable list the bare keys
        // mean scroll, and iced routes them to the focused scrollable.
        Key::Named(Named::ArrowUp) if ctrl => Some(Message::VolumeChanged(
            (f64::from(ctx.volume) + VOLUME_STEP).min(100.0),
        )),
        Key::Named(Named::ArrowDown) if ctrl => Some(Message::VolumeChanged(
            (f64::from(ctx.volume) - VOLUME_STEP).max(0.0),
        )),
        Key::Character(c) if ctrl && c.as_str().eq_ignore_ascii_case("f") => {
            Some(Message::FocusSearch)
        }

        // Escape isn't a text character, so it needs no gate.
        Key::Named(Named::Escape) => Some(Message::GoBack),

        // --- bare keys: only where nothing can be typed ---
        Key::Named(Named::Space) if bare_ok => Some(if ctx.is_playing {
            Message::Pause
        } else {
            Message::Play
        }),
        Key::Named(Named::ArrowRight) if bare_ok => Some(Message::SeekTo(
            (ctx.elapsed + SEEK_STEP).min(ctx.duration.max(0.0)),
        )),
        Key::Named(Named::ArrowLeft) if bare_ok => {
            Some(Message::SeekTo((ctx.elapsed - SEEK_STEP).max(0.0)))
        }
        Key::Character(c) if bare_ok && c.as_str() == "/" => Some(Message::FocusSearch),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(view: View) -> Context {
        Context {
            view,
            is_playing: true,
            elapsed: 30.0,
            duration: 200.0,
            volume: 50,
        }
    }

    fn named(n: Named) -> Key {
        Key::Named(n)
    }

    fn ch(c: &str) -> Key {
        Key::Character(c.into())
    }

    /// **The acceptance criterion for the whole feature.** A shortcut that
    /// eats a keystroke someone meant as text is worse than no shortcut.
    #[test]
    fn bare_keys_never_fire_on_a_view_with_a_text_input() {
        for view in [
            View::Search,
            View::Settings,
            View::Radio,
            View::CD,
            View::Partitions,
            View::Playlists,
            View::AddToPlaylist,
        ] {
            let c = ctx(view.clone());
            for key in [
                named(Named::Space),
                named(Named::ArrowLeft),
                named(Named::ArrowRight),
                ch("/"),
            ] {
                assert!(
                    resolve(&key, Modifiers::empty(), &c).is_none(),
                    "{key:?} must be text on {view:?}"
                );
            }
        }
    }

    #[test]
    fn space_toggles_playback_everywhere_else() {
        for view in [View::NowPlaying, View::Queue, View::Albums, View::Browser] {
            let mut c = ctx(view.clone());
            assert!(matches!(
                resolve(&named(Named::Space), Modifiers::empty(), &c),
                Some(Message::Pause)
            ));
            c.is_playing = false;
            assert!(matches!(
                resolve(&named(Named::Space), Modifiers::empty(), &c),
                Some(Message::Play)
            ));
        }
    }

    /// Modified shortcuts are safe by construction, so they keep working where
    /// bare ones can't — including on the views full of text boxes.
    #[test]
    fn modified_shortcuts_work_even_where_text_is_typed() {
        let c = ctx(View::Settings);
        assert!(matches!(
            resolve(&named(Named::ArrowRight), Modifiers::COMMAND, &c),
            Some(Message::Next)
        ));
        assert!(matches!(
            resolve(&named(Named::ArrowLeft), Modifiers::COMMAND, &c),
            Some(Message::Previous)
        ));
        assert!(matches!(
            resolve(&ch("f"), Modifiers::COMMAND, &c),
            Some(Message::FocusSearch)
        ));
    }

    #[test]
    fn seeking_is_clamped_to_the_track() {
        let mut c = ctx(View::NowPlaying);
        c.elapsed = 2.0;
        match resolve(&named(Named::ArrowLeft), Modifiers::empty(), &c) {
            Some(Message::SeekTo(t)) => assert_eq!(t, 0.0, "must not seek before the start"),
            other => panic!("expected SeekTo, got {other:?}"),
        }
        c.elapsed = 199.0;
        match resolve(&named(Named::ArrowRight), Modifiers::empty(), &c) {
            Some(Message::SeekTo(t)) => assert_eq!(t, 200.0, "must not seek past the end"),
            other => panic!("expected SeekTo, got {other:?}"),
        }
    }

    #[test]
    fn volume_is_clamped_to_its_range() {
        let mut c = ctx(View::NowPlaying);
        c.volume = 98;
        match resolve(&named(Named::ArrowUp), Modifiers::COMMAND, &c) {
            Some(Message::VolumeChanged(v)) => assert_eq!(v, 100.0),
            other => panic!("expected VolumeChanged, got {other:?}"),
        }
        c.volume = 2;
        match resolve(&named(Named::ArrowDown), Modifiers::COMMAND, &c) {
            Some(Message::VolumeChanged(v)) => assert_eq!(v, 0.0),
            other => panic!("expected VolumeChanged, got {other:?}"),
        }
    }

    /// Escape is not a text character, so it is deliberately ungated — going
    /// back from a view with a search box in it is a reasonable thing to want.
    #[test]
    fn escape_goes_back_from_anywhere() {
        for view in [View::Search, View::Settings, View::NowPlaying] {
            assert!(matches!(
                resolve(&named(Named::Escape), Modifiers::empty(), &ctx(view)),
                Some(Message::GoBack)
            ));
        }
    }

    #[test]
    fn unbound_keys_are_ignored() {
        let c = ctx(View::NowPlaying);
        assert!(resolve(&ch("q"), Modifiers::empty(), &c).is_none());
        assert!(resolve(&named(Named::Tab), Modifiers::empty(), &c).is_none());
    }
}
