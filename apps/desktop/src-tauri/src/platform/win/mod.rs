mod media;
mod window;

use std::path::Path;

pub use media::{MediaController, spawn_media_watcher};
pub use window::{
    on_deactivated, prepare_island_window, release_keyboard, set_hit_region, show_without_activation, take_keyboard,
};

/// Opens the folder in Explorer.
pub fn reveal_folder(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer").arg(path).spawn().map(|_| ())
}
