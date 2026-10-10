//! The island overlay window: a transparent, full-width strip along the top edge of the
//! primary monitor. Only the rectangles reported by the UI receive input; everything else
//! falls through to the windows below.

use std::sync::OnceLock;

use serde::Deserialize;
use tauri::{App, Emitter, Manager, PhysicalSize, WebviewWindow, WindowEvent};

pub const ISLAND_LABEL: &str = "island";
/// Sent to the island page when its window stops being the active one.
const EVENT_DEACTIVATED: &str = "island://deactivated";

/// Height of the overlay strip in logical pixels. It must fit the tallest expanded state
/// together with its drop shadow.
const STRIP_HEIGHT: f64 = 520.0;

/// Interactive area until the UI reports its own: roughly the collapsed pill.
const INITIAL_HIT_SIZE: (f64, f64) = (360.0, 40.0);

/// The island window, for `displays_changed`.
static WINDOW: OnceLock<WebviewWindow> = OnceLock::new();

/// The window is created visible (at the primary monitor's origin, fully transparent): the
/// WebView only creates the child windows that receive file drops once it is shown, and the
/// drop handler is attached when the WebView is created.
pub fn setup(app: &mut App) -> tauri::Result<()> {
    let window = app
        .get_webview_window(ISLAND_LABEL)
        .expect("the island window is declared in tauri.conf.json");
    place_on_primary_monitor(&window)?;
    let _ = WINDOW.set(window.clone());
    crate::platform::prepare_island_window(&window)?;
    // A new scale keeps the window's logical size, and a moved window may no longer cover the
    // primary monitor, so the strip is fitted again after either.
    window.on_window_event(|event| {
        if matches!(event, WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Moved(_)) {
            displays_changed();
        }
    });
    let app_handle = app.handle().clone();
    crate::platform::on_deactivated(move || {
        if let Err(err) = app_handle.emit_to(ISLAND_LABEL, EVENT_DEACTIVATED, ()) {
            log::warn!("failed to announce that the island lost focus: {err}");
        }
    });

    let scale = window.scale_factor()?;
    let width = f64::from(window.outer_size()?.width) / scale;
    let (hit_width, hit_height) = INITIAL_HIT_SIZE;
    let initial = Rect { x: (width - hit_width) / 2.0, y: 0.0, width: hit_width, height: hit_height };
    crate::platform::set_hit_region(&window, &[initial], scale)?;
    crate::platform::show_without_activation(&window)?;
    Ok(())
}

/// Fits the strip to the primary monitor again, which may have changed its resolution, scale or
/// place, or be another monitor now. Also called by the window procedure on WM_DISPLAYCHANGE.
/// The work is done off the main thread, so it comes after the change the main thread is busy
/// with: on a new scale, Tauri resizes the window to its old logical size right after the event.
pub fn displays_changed() {
    let Some(window) = WINDOW.get().cloned() else { return };
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(err) = place_on_primary_monitor(&window) {
            log::warn!("could not fit the island to the primary monitor: {err}");
        }
    });
}

fn place_on_primary_monitor(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.primary_monitor()? else {
        log::warn!("no primary monitor reported, keeping the default window placement");
        return Ok(());
    };
    let position = *monitor.position();
    let size = PhysicalSize::new(monitor.size().width, (STRIP_HEIGHT * monitor.scale_factor()).round() as u32);
    // Moving the window reports back as a window event, so only a change is applied.
    if window.outer_position()? != position {
        window.set_position(position)?;
    }
    if window.inner_size()? != size {
        window.set_size(size)?;
    }
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

/// Gives the island the keyboard while a text field in it is in use (`take: true`), and hands
/// the focus back to the previously active window afterwards. The rest of the time the island
/// never takes focus. Synchronous, so it runs on the main thread like the hit region.
#[tauri::command]
pub fn island_keyboard(window: tauri::WebviewWindow, take: bool) -> Result<(), String> {
    let result = if take {
        crate::platform::take_keyboard(&window)
    } else {
        crate::platform::release_keyboard(&window)
    };
    result.map_err(|e| e.to_string())
}

/// Replaces the interactive area of the island window. This is a synchronous command, so it
/// runs on the main thread, which owns the window.
#[tauri::command]
pub fn island_set_hit_region(window: tauri::WebviewWindow, rects: Vec<Rect>) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    crate::platform::set_hit_region(&window, &rects, scale).map_err(|e| e.to_string())
}
