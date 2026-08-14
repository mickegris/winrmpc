//! Shared row state for song lists — which row is playing, and how it looks.
//!
//! Before this existed, exactly one view highlighted the playing track (the
//! Queue), with the styling inline. Six views now need the same state, so the
//! styling lives here rather than being copy-pasted five more times.
//!
//! # The match rule, and why it differs by view
//!
//! **The Queue matches on queue *position*; every other list matches on the
//! song's *URI*.** This is the one design decision in
//! docs/plans/current-song-highlighting.md, and getting it wrong produces
//! *wrong* highlighting rather than missing highlighting.
//!
//! - A queue can legitimately hold the same file twice. Position is the only
//!   thing that distinguishes those two entries, and MPD hands it to us
//!   directly as `status.song_pos`. So the Queue uses [`is_current_pos`].
//! - Every other list — an album's tracks, a stored playlist, search results,
//!   the file browser, play history — is a *library* listing, not the queue.
//!   Its rows have no queue position at all. `playlist_detail`'s `pos` is the
//!   **playlist** index, which bears no relationship to `status.song_pos`, so
//!   comparing them would light up an arbitrary unrelated row. Those views use
//!   [`is_current_uri`].
//!
//! Two consequences, both accepted rather than worked around:
//!
//! - The same track appearing twice in one album or playlist highlights
//!   **both**. Correct, and rare enough not to complicate the rule.
//! - A stopped player still highlights, because MPD keeps a current song when
//!   stopped and the Queue already behaved this way. The player bar is what
//!   communicates play/pause/stop.

use crate::ui::message::Message;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use iced::widget::{container, text};
use iced::{Color, Element, Length};

/// Width of the leading now-playing marker cell.
///
/// The marker is a *fixed-width cell that is either the glyph or blank*, never
/// a widget pushed in only when current — otherwise every other column in the
/// row would shift depending on what happens to be playing.
pub const MARKER_WIDTH: u16 = 14;

/// Width of one row-action icon button.
///
/// Derived, not measured by eye: every glyph in the bundled font has an
/// advance of exactly one em (`icon::tests::every_glyph_advances_exactly_one_em`
/// asserts it), so at [`icon::SIZE`] the glyph is 16px, and `padding([2, 8])`
/// adds 8 on each side.
pub const ACTION_BTN_WIDTH: u16 = icon::SIZE + 8 + 8;

/// Spacing between adjacent row-action buttons.
pub const ACTION_SPACING: u16 = 2;

/// Total width of a row-action group of `n` buttons.
///
/// A song list's columns are `FillPortion`s laid out *before* the action
/// group, so the group's width decides where every column lands. Any header
/// row that has to line up with the data rows must reserve exactly this much
/// — see the Queue, whose header had no action column at all and therefore
/// spread its labels ~134px wider than the rows beneath them.
pub const fn action_group_width(n: u16) -> u16 {
    n * ACTION_BTN_WIDTH + n.saturating_sub(1) * ACTION_SPACING
}

/// Width of the leading track/position number column.
pub const NUMBER_WIDTH: u16 = 30;

/// The leading number cell.
///
/// **Right-aligned**, so a list running past nine lines its numbers up on the
/// units digit instead of letting `10` hang a character left of `9`.
///
/// Every track list puts this immediately after [`playing_marker`] and before
/// the title, with the actions at the far right — the reading order is
/// number → title → time, and the controls are secondary to all three. The
/// library views used to lead with four glyph buttons, which put ~134px of
/// identical controls between the left edge and the first thing anyone is
/// actually looking for.
pub fn number<'a>(label: impl text::IntoFragment<'a>, size: u16) -> Element<'a, Message> {
    text(label)
        .size(size)
        .width(Length::Fixed(NUMBER_WIDTH as f32))
        .align_x(iced::alignment::Horizontal::Right)
        .color(AppColors::TEXT_MUTED)
        .into()
}

/// Width of the trailing duration column. Fits `mm:ss` with room for a
/// three-digit minute count.
pub const DURATION_WIDTH: u16 = 55;

/// The trailing duration cell, right-aligned and **fixed width**.
///
/// The width is load-bearing now that the action group follows it: sized to
/// content, a row reading `4:53` would be four pixels narrower than one
/// reading `10:53`, and every action button below it would sit slightly off.
pub fn duration<'a>(label: impl text::IntoFragment<'a>, size: u16) -> Element<'a, Message> {
    text(label)
        .size(size)
        .width(Length::Fixed(DURATION_WIDTH as f32))
        .align_x(iced::alignment::Horizontal::Right)
        .color(AppColors::TEXT_MUTED)
        .into()
}

/// Does this library-list row hold the currently playing track?
///
/// Compares URIs. See the module docs for why this is not a position compare
/// outside the Queue.
pub fn is_current_uri(row_file: &str, current_file: Option<&str>) -> bool {
    current_file == Some(row_file)
}

/// Does this *queue* row hold the currently playing track?
///
/// Compares queue positions, which is correct only for the queue itself.
pub fn is_current_pos(row_pos: u32, current_pos: Option<u32>) -> bool {
    current_pos == Some(row_pos)
}

/// Background for a list row, accounting for zebra striping and playing state.
pub fn row_bg(index: usize, is_current: bool) -> Color {
    if is_current {
        AppColors::ROW_PLAYING
    } else if index % 2 == 0 {
        AppColors::ROW_EVEN
    } else {
        AppColors::ROW_ODD
    }
}

/// Title colour for a list row.
pub fn title_color(is_current: bool) -> Color {
    if is_current {
        AppColors::ACCENT
    } else {
        AppColors::TEXT_PRIMARY
    }
}

/// The leading now-playing marker: a fixed-width cell that is either the play
/// glyph or blank, so rows stay aligned either way.
pub fn playing_marker<'a>(is_current: bool) -> Element<'a, Message> {
    let inner: Element<'a, Message> = if is_current {
        icon::icon_sized(icon::PLAY, 11)
            .color(AppColors::ACCENT)
            .into()
    } else {
        iced::widget::Space::with_width(0).into()
    };
    container(inner)
        .width(Length::Fixed(MARKER_WIDTH as f32))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_match_marks_only_the_playing_track() {
        let current = Some("music/a.flac");
        assert!(is_current_uri("music/a.flac", current));
        assert!(!is_current_uri("music/b.flac", current));
    }

    #[test]
    fn nothing_is_current_when_nothing_is_playing() {
        assert!(!is_current_uri("music/a.flac", None));
        assert!(!is_current_pos(0, None));
    }

    /// **The exact bug this plan was written to avoid.** A stored playlist's
    /// row index is a *playlist* index; the queue's `song_pos` is a *queue*
    /// index. A row whose playlist position happens to equal the playing
    /// queue position must not light up unless it is genuinely the same file.
    #[test]
    fn a_playlist_row_at_the_playing_queue_position_does_not_match_by_position() {
        let playing_queue_pos = 3u32;
        let playing_file = "music/actually-playing.flac";

        // Row 3 of the playlist holds a completely different track.
        let row_pos = 3u32;
        let row_file = "music/something-else.flac";

        // A position compare — the wrong rule for this view — would match.
        assert!(is_current_pos(row_pos, Some(playing_queue_pos)));
        // The rule these views actually use does not.
        assert!(!is_current_uri(row_file, Some(playing_file)));
    }

    #[test]
    fn duplicate_uris_in_one_listing_both_match() {
        // Accepted consequence of the URI rule, asserted so it stays a
        // decision rather than becoming a surprise.
        let current = Some("music/a.flac");
        assert!(is_current_uri("music/a.flac", current));
        assert!(is_current_uri("music/a.flac", current));
    }

    #[test]
    fn queue_positions_distinguish_the_same_file_twice() {
        // The reason the Queue keeps a position compare: both rows are the
        // same file, only one is playing.
        assert!(is_current_pos(2, Some(2)));
        assert!(!is_current_pos(7, Some(2)));
    }

    #[test]
    fn stream_and_cd_uris_behave_like_any_other() {
        assert!(is_current_uri("http://stream.example/live", Some("http://stream.example/live")));
        assert!(is_current_uri("cdda:///2", Some("cdda:///2")));
        assert!(!is_current_uri("cdda:///2", Some("cdda:///3")));
    }

    #[test]
    fn playing_row_overrides_zebra_striping_in_both_parities() {
        assert_eq!(row_bg(0, true), AppColors::ROW_PLAYING);
        assert_eq!(row_bg(1, true), AppColors::ROW_PLAYING);
        assert_eq!(row_bg(0, false), AppColors::ROW_EVEN);
        assert_eq!(row_bg(1, false), AppColors::ROW_ODD);
    }

    #[test]
    fn the_playing_row_colour_is_distinct_from_both_stripes() {
        // A highlight equal to either stripe is invisible on half the rows.
        assert_ne!(AppColors::ROW_PLAYING, AppColors::ROW_EVEN);
        assert_ne!(AppColors::ROW_PLAYING, AppColors::ROW_ODD);
        assert_ne!(AppColors::ROW_PLAYING, AppColors::BG_HOVER);
    }

    #[test]
    fn the_action_group_width_matches_its_buttons() {
        // The Queue's header reserves `action_group_width(4)` and its rows
        // render four buttons at `ACTION_BTN_WIDTH` with `ACTION_SPACING`
        // between them. If those two ever disagree the header's FillPortion
        // columns get a different share of the row than the data's do, and
        // every label drifts sideways — which is exactly what happened before
        // the header reserved anything at all.
        assert_eq!(
            action_group_width(4),
            4 * ACTION_BTN_WIDTH + 3 * ACTION_SPACING
        );
        assert_eq!(action_group_width(1), ACTION_BTN_WIDTH, "no trailing gap");
        assert_eq!(action_group_width(0), 0);
    }

    #[test]
    fn an_action_button_is_the_glyph_plus_its_padding() {
        // Guards the derivation rather than the number: 8px padding either
        // side of a glyph that is exactly `icon::SIZE` wide, which
        // `icon::tests::every_glyph_advances_exactly_one_em` keeps true.
        assert_eq!(ACTION_BTN_WIDTH, icon::SIZE + 16);
    }

    #[test]
    fn title_colour_changes_with_playing_state() {
        assert_eq!(title_color(true), AppColors::ACCENT);
        assert_eq!(title_color(false), AppColors::TEXT_PRIMARY);
    }
}
