//! The file tray keeps references to files dropped onto the island. The UI shows previews
//! through the asset protocol, which only serves files that were explicitly allowed here.

use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn file_tray_allow(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    for path in paths {
        scope.allow_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
