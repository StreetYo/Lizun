//! The island overlay window: a transparent, full-width strip along the top edge of the
//! primary monitor. Only the rectangles reported by the UI receive input; everything else
//! falls through to the windows below.

use serde::Deserialize;
use tauri::{App, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

pub const ISLAND_LABEL: &str = "island";

/// Height of the overlay strip in logical pixels. It must fit the tallest expanded state
/// together with its drop shadow.
const STRIP_HEIGHT: f64 = 420.0;

/// Interactive area until the UI reports its own: roughly the collapsed pill.
const INITIAL_HIT_SIZE: (f64, f64) = (360.0, 40.0);

/// The window is created visible (at the primary monitor's origin, fully transparent): the
/// WebView only creates the child windows that receive file drops once it is shown, and the
/// drop handler is attached when the WebView is created.
pub fn setup(app: &mut App) -> tauri::Result<()> {
    let window = app
        .get_webview_window(ISLAND_LABEL)
        .expect("the island window is declared in tauri.conf.json");
    place_on_primary_monitor(&window)?;
    crate::platform::prepare_island_window(&window)?;

    let scale = window.scale_factor()?;
    let width = f64::from(window.outer_size()?.width) / scale;
    let (hit_width, hit_height) = INITIAL_HIT_SIZE;
    let initial = Rect { x: (width - hit_width) / 2.0, y: 0.0, width: hit_width, height: hit_height };
    crate::platform::set_hit_region(&window, &[initial], scale)?;
    crate::platform::show_without_activation(&window)?;
    Ok(())
}

fn place_on_primary_monitor(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.primary_monitor()? else {
        log::warn!("no primary monitor reported, keeping the default window placement");
        return Ok(());
    };
    let height = (STRIP_HEIGHT * monitor.scale_factor()).round() as u32;
    window.set_size(PhysicalSize::new(monitor.size().width, height))?;
    window.set_position(PhysicalPosition::new(monitor.position().x, monitor.position().y))?;
    Ok(())
}

/// A rectangle in logical (CSS) pixels relative to the window's top-left corner.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Replaces the interactive area of the island window. This is a synchronous command, so it
/// runs on the main thread, which owns the window.
#[tauri::command]
pub fn island_set_hit_region(window: tauri::WebviewWindow, rects: Vec<Rect>) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    crate::platform::set_hit_region(&window, &rects, scale).map_err(|e| e.to_string())
}
