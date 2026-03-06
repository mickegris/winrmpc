#![allow(dead_code, unused_imports)]
mod mpd;
mod ui;
mod art;
mod config;

use tracing_subscriber::EnvFilter;
use ui::app::App;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("winrmpc=info")),
        )
        .init();

    tracing::info!("Starting winrmpc...");

    iced::application("winrmpc", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size(iced::Size::new(1200.0, 800.0))
        .run_with(App::new)
}
