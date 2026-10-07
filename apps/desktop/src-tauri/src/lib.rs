mod file_tray;
mod island;
mod media;
mod platform;
mod preferences;
mod settings;
mod system_tray;
mod widgets;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Registered first so that a second launch exits before it creates another island.
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_autostart::Builder::new().build())
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
            media::media_snapshot,
            media::media_artwork,
            media::media_control,
            file_tray::file_tray_allow,
            widgets::widgets_list,
            widgets::widgets_reload,
            widgets::widgets_open_folder,
            settings::settings_open,
            settings::app_info,
            settings::app_quit,
            preferences::preferences_get,
            preferences::preferences_set,
        ])
        .setup(|app| {
            preferences::init(app.handle());
            widgets::init(app.handle());
            island::setup(app)?;
            system_tray::setup(app)?;
            media::start(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Lizun");
}
