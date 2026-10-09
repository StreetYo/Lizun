mod calendar;
mod file_tray;
mod http;
mod ics;
mod island;
mod media;
mod platform;
mod preferences;
mod settings;
mod system_tray;
mod widget_install;
mod widget_settings;
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
        .invoke_handler(tauri::generate_handler![
            island::island_set_hit_region,
            island::island_keyboard,
            media::media_snapshot,
            media::media_artwork,
            media::media_control,
            media::media_players,
            media::media_select,
            browser::browser_media_status,
            calendar::calendar_snapshot,
            file_tray::file_tray_allow,
            widgets::widgets_list,
            widgets::widgets_reload,
            widgets::widgets_open_folder,
            widget_install::widgets_install_prepare,
            widget_install::widgets_install_confirm,
            widget_install::widgets_install_cancel,
            widget_install::widgets_uninstall,
            widget_settings::widget_settings_built_in,
            widget_settings::widget_settings_all,
            widget_settings::widget_settings_set,
            widget_settings::widget_settings_reset,
            settings::settings_open,
            settings::app_info,
            settings::app_quit,
            preferences::preferences_get,
            preferences::preferences_set,
        ])
        .setup(|app| {
            preferences::init(app.handle());
            widgets::init(app.handle());
            widget_install::init(app.handle());
            widget_settings::init(app.handle());
            calendar::start(app.handle());
            island::setup(app)?;
            system_tray::setup(app)?;
            media::start(app.handle());
            let preferences = preferences::current(app.handle());
            browser::start(app.handle(), preferences.browser_media, preferences.browser_media_port);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Lizun");
}
