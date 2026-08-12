#![allow(dead_code, unused_imports)]
// Hide the console window on Windows release builds.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod mpd;
mod ui;
mod art;
mod config;
mod logger;
mod icon;
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
        .window(iced::window::Settings {
            size: iced::Size::new(1200.0, 800.0),
            // Below roughly this, Now Playing stops fitting: the art (300px)
            // plus the song info plus the lyrics pane run out of width, and
            // the recently-played strip runs out of height and starts
            // colliding with the player bar. iced widgets don't clip their
            // parent, so "too small" doesn't degrade gracefully — it overlaps.
            min_size: Some(iced::Size::new(1000.0, 700.0)),
            icon: icon::make_icon(),
            ..Default::default()
        })
        .run_with(App::new)
}
