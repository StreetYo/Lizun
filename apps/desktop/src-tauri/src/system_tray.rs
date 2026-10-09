//! Notification-area icon. The island has no taskbar button, so this is how the user hides
//! the island or quits the app.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, Wry};

use crate::i18n::t;
use crate::island::ISLAND_LABEL;

/// The menu's items, kept to relabel them when the language changes.
struct TrayMenu {
    settings: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", t("tray.settings"), true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", t("tray.hideIsland"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&settings, &toggle, &separator, &quit])?;
    app.manage(TrayMenu { settings, toggle, quit });

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
            if let Err(err) = toggle_island(app) {
                log::warn!("could not toggle the island: {err}");
            }
        }
        _ => {}
    })
    .build(app)?;
    Ok(())
}

fn toggle_island(app: &AppHandle) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(ISLAND_LABEL) else {
        return Ok(());
    };
    if window.is_visible()? {
        window.hide()?;
    } else {
        crate::platform::show_without_activation(&window)?;
    }
    relabel(app);
    Ok(())
}

/// Labels the menu in the current language; the toggle says what it will do to the island.
pub fn relabel(app: &AppHandle) {
    let Some(menu) = app.try_state::<TrayMenu>() else { return };
    let visible = app.get_webview_window(ISLAND_LABEL).is_none_or(|window| window.is_visible().unwrap_or(true));
    let toggle = if visible { "tray.hideIsland" } else { "tray.showIsland" };
    for (item, key) in [(&menu.settings, "tray.settings"), (&menu.toggle, toggle), (&menu.quit, "tray.quit")] {
        if let Err(err) = item.set_text(t(key)) {
            log::warn!("could not relabel the tray menu: {err}");
        }
    }
}
