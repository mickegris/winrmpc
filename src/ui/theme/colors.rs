use iced::Color;

pub struct AppColors;

impl AppColors {
    // Background hierarchy (dark theme)
    pub const BG_PRIMARY: Color = Color::from_rgb(0.11, 0.11, 0.14);
    pub const BG_SECONDARY: Color = Color::from_rgb(0.14, 0.14, 0.18);
    pub const BG_TERTIARY: Color = Color::from_rgb(0.18, 0.18, 0.22);
    pub const BG_HOVER: Color = Color::from_rgb(0.22, 0.22, 0.28);

    // List row alternating
    pub const ROW_EVEN: Color = Color::from_rgb(0.13, 0.13, 0.16);
    pub const ROW_ODD: Color = Color::from_rgb(0.15, 0.15, 0.19);

    // Text
    pub const TEXT_PRIMARY: Color = Color::from_rgb(0.93, 0.93, 0.95);
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.62, 0.62, 0.68);
    pub const TEXT_MUTED: Color = Color::from_rgb(0.42, 0.42, 0.48);

    // Accent
    pub const ACCENT: Color = Color::from_rgb(0.31, 0.76, 0.97);
    pub const ACCENT_DIM: Color = Color::from_rgb(0.20, 0.50, 0.65);

    // Semantic
    pub const SUCCESS: Color = Color::from_rgb(0.30, 0.85, 0.50);
    pub const WARNING: Color = Color::from_rgb(0.95, 0.75, 0.25);
    pub const ERROR: Color = Color::from_rgb(0.90, 0.30, 0.30);

    // Borders
    pub const BORDER: Color = Color::from_rgb(0.25, 0.25, 0.30);

    // Player bar
    pub const PROGRESS_BG: Color = Color::from_rgb(0.20, 0.20, 0.25);
    pub const PROGRESS_FILL: Color = Color::from_rgb(0.31, 0.76, 0.97);
}
