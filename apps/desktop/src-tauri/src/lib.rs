mod file_tray;
mod island;
mod media;
mod platform;
mod system_tray;
mod widgets;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Registered first so that a second launch exits before it creates another island.
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
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
        ])
        .setup(|app| {
            widgets::init(app.handle());
            island::setup(app)?;
            system_tray::setup(app)?;
            media::start(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Lizun");
}
