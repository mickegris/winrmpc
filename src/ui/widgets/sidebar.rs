use crate::config::MpdServer;
use crate::ui::message::{Message, View};
use crate::ui::theme::AppColors;
use iced::widget::{scrollable, button, column, container, pick_list, text, Space};
use iced::{Alignment, Element, Length};

/// Sidebar column width. Sized to hold the longest nav label
/// ("Recently Added") on one line at text size 11, plus the slim scrollbar.
const SIDEBAR_WIDTH: u16 = 132;

pub fn view<'a>(
    current_view: &View,
    connected: bool,
    mpd_addr: &str,
    servers: &'a [MpdServer],
    active_server: &'a str,
) -> Element<'a, Message> {
    let status_text = if connected {
        text(format!("Connected\n{mpd_addr}"))
            .size(10)
            .color(AppColors::SUCCESS)
    } else {
        text("Not connected")
            .size(10)
            .color(AppColors::ERROR)
    };

    // Server picker — only shown when there are multiple servers.
    let server_picker: Element<'a, Message> = if servers.len() > 1 {
        let names: Vec<String> = servers.iter().map(|s| s.name.clone()).collect();
        pick_list(names, Some(active_server.to_string()), Message::SwitchServer)
            .width(Length::Fill)
            .text_size(11)
            .padding([4, 8])
            .into()
    } else {
        Space::with_height(0).into()
    };

    let version_text = text(concat!("v", env!("CARGO_PKG_VERSION")))
        .size(9)
        .color(AppColors::TEXT_MUTED);

    let nav = column![
            Space::with_height(12),
            container(status_text).center_x(Length::Fill),
            Space::with_height(4),
            server_picker,
            Space::with_height(4),
            container(version_text).center_x(Length::Fill),
            Space::with_height(12),
            nav_button("Now Playing", View::NowPlaying, current_view),
            nav_button("Queue", View::Queue, current_view),
            nav_button("Artists", View::Artists, current_view),
            nav_button("Albums", View::Albums, current_view),
            nav_button("Genres", View::Genres, current_view),
            nav_button("Recently Added", View::RecentlyAdded, current_view),
            nav_button("Playlists", View::Playlists, current_view),
            nav_button("Browse", View::Browser, current_view),
            nav_button("Search", View::Search, current_view),
            nav_button("Radio", View::Radio, current_view),
            nav_button("CD", View::CD, current_view),
            // Fixed gap rather than Length::Fill: with a Fill here the
            // bottom group (…Settings/Log/Stats) got pushed off the bottom
            // of a shorter window with no way to reach it, since the sidebar
            // doesn't scroll on its own.
            Space::with_height(14),
            nav_button("Outputs", View::Outputs, current_view),
            nav_button("Partitions", View::Partitions, current_view),
            nav_button("Snapcast", View::Snapcast, current_view),
            nav_button("Settings", View::Settings, current_view),
            nav_button("Log", View::Log, current_view),
            nav_button("Stats", View::ServerStats, current_view),
            Space::with_height(8),
        ]
        .spacing(1)
        .align_x(Alignment::Center)
        .width(Length::Fill);

    // Scrollable so every entry stays reachable no matter how short the
    // window is — there are 17 of them. The scrollbar is slimmed down and
    // given no margin because at this width the default 10px bar plus its
    // margin is a meaningful slice of the label area.
    container(
        scrollable(nav)
            .height(Length::Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(4)
                    .scroller_width(4)
                    .margin(0),
            )),
    )
    // 90px was too narrow for the labels it has to hold: "Recently Added"
    // wrapped to two lines and the connection line ("Connected" over
    // host:port) was clipped mid-word.
    .width(SIDEBAR_WIDTH)
    .height(Length::Fill)
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(AppColors::BG_SECONDARY.into()),
        ..Default::default()
    })
    .into()
}

fn nav_button<'a>(
    label: &str,
    target: View,
    current_view: &View,
) -> Element<'a, Message> {
    let is_active = current_view == &target;

    let bg = if is_active {
        AppColors::BG_TERTIARY
    } else {
        AppColors::BG_SECONDARY
    };
    let fg = if is_active {
        AppColors::ACCENT
    } else {
        AppColors::TEXT_SECONDARY
    };

    button(
        container(
            text(label.to_string())
                .size(12)
                .color(fg)
                .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .padding([9, 8]),
    )
    .on_press(Message::NavigateTo(target))
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme, _status| button::Style {
        background: Some(bg.into()),
        text_color: fg,
        border: iced::Border::default(),
        ..Default::default()
    })
    .into()
}
