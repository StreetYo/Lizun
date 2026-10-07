//! Settings window, opened from the island's gear button or the tray menu.

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_LABEL: &str = "settings";

/// Shows the settings window, creating it on first use.
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
        .title("Lizun Settings")
        .inner_size(720.0, 500.0)
        .min_inner_size(600.0, 420.0)
        .center()
        .theme(Some(tauri::Theme::Dark))
        .build()?
        .set_focus()
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
