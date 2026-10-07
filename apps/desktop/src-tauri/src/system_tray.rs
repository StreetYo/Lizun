//! Notification-area icon. The island has no taskbar button, so this is how the user hides
//! the island or quits the app.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager};

use crate::island::ISLAND_LABEL;

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Hide island", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Lizun", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&settings, &toggle, &separator, &quit])?;

    let mut tray = TrayIconBuilder::with_id("lizun").tooltip("Lizun").menu(&menu);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(move |app, event| match event.id().as_ref() {
        "quit" => app.exit(0),
        "settings" => {
            // Built off the event loop thread, like the settings_open command.
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = crate::settings::open(&app) {
                    log::warn!("could not open settings: {err}");
                }
            });
        }
        "toggle" => {
            if let Err(err) = toggle_island(app, &toggle) {
                log::warn!("could not toggle the island: {err}");
            }
        }
        _ => {}
    })
    .build(app)?;
    Ok(())
}

fn toggle_island(app: &AppHandle, item: &MenuItem<tauri::Wry>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(ISLAND_LABEL) else {
        return Ok(());
    };
    if window.is_visible()? {
        window.hide()?;
        item.set_text("Show island")?;
    } else {
        crate::platform::show_without_activation(&window)?;
        item.set_text("Hide island")?;
    }
    Ok(())
}
