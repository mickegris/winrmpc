//! Helper to convert raw image bytes to an iced image Handle.

use iced::widget::image::Handle;

pub fn bytes_to_handle(data: &[u8]) -> Option<Handle> {
    // iced's image feature can load from bytes directly
    if data.is_empty() {
        None
    } else {
        Some(Handle::from_bytes(data.to_vec()))
    }
}
