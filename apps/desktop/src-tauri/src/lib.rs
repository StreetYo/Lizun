mod calendar;
mod clipboard;
mod file_tray;
mod hotkeys;
mod http;
mod i18n;
mod ics;
mod island;
mod media;
mod platform;
mod preferences;
mod settings;
mod system_stats;
mod system_tray;
mod updater;
mod widget_actions;
mod widget_apps;
mod widget_install;
mod widget_network;
mod widget_settings;
mod widget_storage;
mod widgets;
mod browser;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Registered first so that a second launch exits before it creates another island.
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .register_uri_scheme_protocol(widgets::SCHEME, widgets::serve)
        .manage(widget_storage::Writes::default())
        .manage(widget_apps::Background::default())
        .manage(system_stats::Sampler::default())
        .invoke_handler(tauri::generate_handler![
            island::island_set_hit_region,
            island::island_keyboard,
            media::media_snapshot,
            media::media_artwork,
            media::media_control,
            media::media_seek,
            media::media_players,
            media::media_select,
            browser::browser_media_status,
            browser::browser_media_open_store,
            calendar::calendar_snapshot,
            clipboard::clipboard_history,
            clipboard::clipboard_copy,
            clipboard::clipboard_remove,
            clipboard::clipboard_clear,
            file_tray::file_tray_allow,
            hotkeys::hotkeys_check,
            hotkeys::hotkeys_pause,
            hotkeys::hotkeys_status,
            widgets::widgets_list,
            widgets::widgets_reload,
            widgets::widgets_open_folder,
            widget_install::widgets_install_prepare,
            widget_install::widgets_install_confirm,
            widget_install::widgets_install_cancel,
            widget_install::widgets_uninstall,
            widget_network::widget_network_request,
            widget_actions::widget_open_link,
            widget_actions::widget_clipboard_write,
            system_stats::widget_system_stats,
            widget_apps::widget_app_start,
            widget_apps::widget_app_show,
            widget_apps::widget_app_hide,
            widget_apps::widget_app_state,
            widget_storage::widget_storage_get,
            widget_storage::widget_storage_set,
            widget_storage::widget_storage_remove,
            widget_settings::widget_settings_built_in,
            widget_settings::widget_settings_all,
            widget_settings::widget_settings_set,
            widget_settings::widget_settings_reset,
            settings::settings_open,
            settings::app_info,
            settings::app_quit,
            i18n::i18n_system_language,
            preferences::preferences_get,
            preferences::preferences_set,
        ])
        .setup(|app| {
            preferences::init(app.handle());
            i18n::apply(app.handle(), &preferences::current(app.handle()).language);
            widgets::init(app.handle());
            widget_install::init(app.handle());
            widget_settings::init(app.handle());
            calendar::start(app.handle());
            clipboard::start(app.handle());
            island::setup(app)?;
            system_tray::setup(app)?;
            media::start(app.handle());
            let preferences = preferences::current(app.handle());
            hotkeys::start(app.handle(), &preferences.hotkeys);
            browser::start(app.handle(), preferences.browser_media, preferences.browser_media_port);
            updater::start(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Lizun")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                widget_apps::close_background(app);
            }
        });
}
