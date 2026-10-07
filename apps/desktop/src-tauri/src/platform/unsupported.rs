//! Fallback for platforms that have no native backend yet (macOS is planned).

use tauri::WebviewWindow;

use crate::island::Rect;
use crate::media::{MediaCommand, Player};

pub fn prepare_island_window(_window: &WebviewWindow) -> tauri::Result<()> {
    Ok(())
}

pub fn show_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()
}

pub fn set_hit_region(_window: &WebviewWindow, _rects: &[Rect], _scale: f64) -> tauri::Result<()> {
    Ok(())
}

pub fn reveal_folder(_path: &std::path::Path) -> std::io::Result<()> {
    Err(std::io::Error::other("opening folders is not supported on this platform yet"))
}

pub struct MediaController;

impl MediaController {
    pub fn send(&self, _player_id: &str, _command: MediaCommand) -> Result<(), String> {
        Err("media control is not supported on this platform yet".into())
    }
}

pub fn spawn_media_watcher(_on_update: impl Fn(Vec<Player>) + Send + 'static) -> Option<MediaController> {
    None
}
