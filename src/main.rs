#![allow(dead_code, unused_imports)]
// Hide the console window on Windows release builds.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod mpd;
mod ui;
mod art;
mod config;
mod logger;
mod net;
mod icon;
mod icon_design;
mod lyrics;
mod store;
mod snapcast;
#[cfg(test)]
mod live_tests;

use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use ui::app::App;

fn main() -> iced::Result {
    // Compose two tracing layers:
    //   1. fmt  — stderr/stdout (useful in dev, silenced in windowless release)
    //   2. InAppLayer — ring-buffer readable from the in-app Log view
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("winrmpc=debug")),
            ),
        )
        .with(
            // Only capture winrmpc-namespace events — no iced/wgpu/winit noise.
            logger::InAppLayer.with_filter(
                EnvFilter::new("winrmpc=info"),
            ),
        )
        .init();

    tracing::info!("Starting winrmpc v{}", env!("CARGO_PKG_VERSION"));

    iced::application("winrmpc", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        // Register the bundled icon font. Without this the row-action glyphs
        // resolve through *system* fonts — which is why they rendered as tofu
        // boxes on every machine without `Segoe UI Symbol`, i.e. all of macOS
        // and Linux. See `ui::widgets::icon`.
        .font(ui::widgets::icon::FONT_BYTES)
        .window(window_settings())
        .run_with(App::new)
}

fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1200.0, 800.0),
        // Below roughly this, Now Playing stops fitting: the art (300px)
        // plus the song info plus the lyrics pane run out of width, and
        // the recently-played strip runs out of height and starts
        // colliding with the player bar. iced widgets don't clip their
        // parent, so "too small" doesn't degrade gracefully — it overlaps.
        min_size: Some(iced::Size::new(1000.0, 700.0)),
        // Windows and X11 take the icon from here. Wayland and macOS ignore it
        // entirely (winit's `set_window_icon` is a no-op on both) and read it
        // from installed packaging instead — see `packaging/linux/` and the
        // table in `src/icon.rs`.
        icon: icon::make_icon(),
        platform_specific: platform_specific(),
        ..Default::default()
    }
}

/// `PlatformSpecific` is a *different type per OS*, so this can't be written
/// once with `..Default::default()` — hence the cfg split.
///
/// Setting `application_id` is what makes the Linux icon work at all under
/// Wayland: the compositor has no per-window icon protocol, so it matches the
/// surface's `app_id` against an installed `.desktop` file and uses that
/// file's `Icon=` key. It was previously left empty, which also left X11's
/// `WM_CLASS` empty and broke window-to-application matching in docks.
#[cfg(target_os = "linux")]
fn platform_specific() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific {
        application_id: icon_design::APP_ID.to_string(),
        ..Default::default()
    }
}

#[cfg(not(target_os = "linux"))]
fn platform_specific() -> iced::window::settings::PlatformSpecific {
    Default::default()
}
