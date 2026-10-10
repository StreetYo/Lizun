//! Settings window, opened from the island's gear button or the tray menu. It opens at the size
//! the user last gave it.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

pub const SETTINGS_LABEL: &str = "settings";
const SIZE_FILE: &str = "settings-window.json";
/// Logical size of the window the first time it opens, and the smallest it can be made.
const DEFAULT_SIZE: (f64, f64) = (900.0, 680.0);
const MIN_SIZE: (f64, f64) = (680.0, 480.0);
/// The size is saved once resizing has paused for this long.
const SAVE_DELAY: Duration = Duration::from_millis(500);

/// The window's last size in logical pixels, kept apart from the preferences: it is not a
/// setting and changes too often to announce to every window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct WindowSize {
    width: f64,
    height: f64,
    #[serde(default)]
    maximized: bool,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self { width: DEFAULT_SIZE.0, height: DEFAULT_SIZE.1, maximized: false }
    }
}

fn size_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(SIZE_FILE))
}

/// The saved size, no smaller than the minimum and no larger than the primary monitor.
fn saved_size(app: &AppHandle) -> WindowSize {
    let mut size = size_path(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<WindowSize>(&text).ok())
        .filter(|size| size.width.is_finite() && size.height.is_finite())
        .unwrap_or_default();
    let screen = app.primary_monitor().ok().flatten().map(|monitor| monitor.size().to_logical::<f64>(monitor.scale_factor()));
    let (max_width, max_height) = screen.map_or((f64::MAX, f64::MAX), |screen| (screen.width, screen.height));
    size.width = size.width.min(max_width).max(MIN_SIZE.0);
    size.height = size.height.min(max_height).max(MIN_SIZE.1);
    size
}

/// Bumped on every resize; a pending save goes ahead only if no resize came after it.
static RESIZES: AtomicU64 = AtomicU64::new(0);

/// Saves the window's size shortly after the user stops resizing it. A minimized window keeps
/// the saved size, and a maximized one keeps the size it goes back to.
fn remember_size(window: &WebviewWindow) {
    let resize = RESIZES.fetch_add(1, Ordering::Relaxed) + 1;
    let window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SAVE_DELAY);
        if RESIZES.load(Ordering::Relaxed) != resize || window.is_minimized().unwrap_or(true) {
            return;
        }
        let app = window.app_handle();
        let maximized = window.is_maximized().unwrap_or(false);
        let size = if maximized {
            WindowSize { maximized, ..saved_size(app) }
        } else {
            let Ok(scale) = window.scale_factor() else { return };
            let Ok(inner) = window.inner_size() else { return };
            let inner = inner.to_logical::<f64>(scale);
            WindowSize { width: inner.width.round(), height: inner.height.round(), maximized }
        };
        let Some(path) = size_path(app) else { return };
        let saved = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, serde_json::to_string(&size).unwrap_or_default()));
        if let Err(err) = saved {
            log::warn!("could not save the settings window size: {err}");
        }
    });
}

/// Shows the settings window, creating it on first use.
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let size = saved_size(app);
    let window = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
        .title(crate::i18n::t("settings.windowTitle"))
        .inner_size(size.width, size.height)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .maximized(size.maximized)
        .center()
        .theme(Some(tauri::Theme::Dark))
        .build()?;
    let watched = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Resized(_) = event {
            remember_size(&watched);
        }
    });
    window.set_focus()
}

/// Gives the open settings window its title in the current language.
pub fn relabel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL)
        && let Err(err) = window.set_title(&crate::i18n::t("settings.windowTitle"))
    {
        log::warn!("could not retitle the settings window: {err}");
    }
}

// Async so the window is built off the main thread: building a WebView inside a synchronous
// command deadlocks on Windows.
#[tauri::command]
pub async fn settings_open(app: AppHandle) -> Result<(), String> {
    open(&app).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    widgets_dir: Option<String>,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        widgets_dir: crate::widgets::user_widgets_dir(&app).map(|dir| dir.display().to_string()),
    }
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}
