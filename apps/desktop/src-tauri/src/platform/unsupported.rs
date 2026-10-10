//! Fallback for platforms that have no native backend yet (macOS is planned).

use tauri::WebviewWindow;

use crate::clipboard::{Clip, Kinds, Picture};
use crate::hotkeys::Shortcut;
use crate::island::Rect;
use crate::media::{MediaCommand, Player};

pub fn prepare_island_window(_window: &WebviewWindow) -> tauri::Result<()> {
    Ok(())
}

pub fn show_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()
}

pub fn take_keyboard(window: &WebviewWindow) -> tauri::Result<()> {
    window.set_focus()
}

pub fn on_deactivated(_callback: impl Fn() + Send + Sync + 'static) {}

pub fn release_keyboard(_window: &WebviewWindow) -> tauri::Result<()> {
    Ok(())
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

pub fn iana_time_zone(_windows_name: &str) -> Option<String> {
    None
}

pub fn current_time_zone() -> Option<String> {
    None
}

pub fn preferred_languages() -> Vec<String> {
    Vec::new()
}

pub fn clipboard_sequence() -> u32 {
    0
}

pub fn read_clipboard(_kinds: Kinds) -> Result<Option<Clip>, ()> {
    Ok(None)
}

pub fn write_clipboard_text(_owner: &WebviewWindow, _text: &str) -> Result<(), String> {
    Err("the clipboard is not supported on this platform yet".into())
}

pub fn write_clipboard_files(_owner: &WebviewWindow, _paths: &[String]) -> Result<(), String> {
    Err("the clipboard is not supported on this platform yet".into())
}

pub fn write_clipboard_picture(_owner: &WebviewWindow, _picture: &Picture, _png: &[u8]) -> Result<(), String> {
    Err("the clipboard is not supported on this platform yet".into())
}

pub struct HotkeyListener;

impl HotkeyListener {
    pub fn set(&self, shortcuts: Vec<Option<Shortcut>>) -> Vec<bool> {
        vec![false; shortcuts.len()]
    }
}

pub fn spawn_hotkey_listener(_on_press: impl Fn(usize) + Send + 'static) -> Option<HotkeyListener> {
    None
}

pub fn find_process(_exe: &std::path::Path) -> Option<u32> {
    None
}

pub fn start_app(_exe: &std::path::Path, _hide_for: Option<std::time::Duration>) -> std::io::Result<u32> {
    Err(std::io::Error::other("starting apps is not supported on this platform yet"))
}

pub fn show_app(_pid: u32) -> bool {
    false
}

pub fn close_app_if_hidden(_pid: u32) -> bool {
    false
}
