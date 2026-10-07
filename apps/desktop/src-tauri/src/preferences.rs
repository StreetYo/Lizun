//! User preferences, stored as JSON in the app config folder. Every change is broadcast to all
//! windows, so the island reacts to the settings window immediately.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT_CHANGED: &str = "preferences://changed";
const FILE_NAME: &str = "preferences.json";

/// How the island looks while it is collapsed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IslandVisibility {
    /// The mini island is always on screen.
    #[default]
    Always,
    /// The mini island shows after the island closes or something changes, then goes into the
    /// edge after `auto_hide_delay_ms`.
    AutoHide,
    /// The mini island is never drawn; pointing at the top edge still opens the island.
    Hidden,
}

/// Allowed range for how long the mini island stays in auto-hide mode.
pub const AUTO_HIDE_DELAY_MS: std::ops::RangeInclusive<u32> = 200..=10_000;

/// Unknown fields are ignored and missing ones take their defaults, so files written by other
/// versions still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub island_visibility: IslandVisibility,
    pub auto_hide_delay_ms: u32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { island_visibility: IslandVisibility::default(), auto_hide_delay_ms: 1000 }
    }
}

impl Preferences {
    fn clamped(mut self) -> Self {
        self.auto_hide_delay_ms =
            self.auto_hide_delay_ms.clamp(*AUTO_HIDE_DELAY_MS.start(), *AUTO_HIDE_DELAY_MS.end());
        self
    }
}

pub struct Store(Mutex<Preferences>);

pub fn init(app: &AppHandle) {
    let preferences = file_path(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| {
            serde_json::from_str::<Preferences>(&text).map(Preferences::clamped).unwrap_or_else(|err| {
                log::warn!("ignoring unreadable {FILE_NAME}: {err}");
                Preferences::default()
            })
        })
        .unwrap_or_default();
    app.manage(Store(Mutex::new(preferences)));
}

fn file_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(FILE_NAME))
}

#[tauri::command]
pub fn preferences_get(store: State<'_, Store>) -> Preferences {
    store.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn preferences_set(
    app: AppHandle,
    store: State<'_, Store>,
    preferences: Preferences,
) -> Result<Preferences, String> {
    let preferences = preferences.clamped();
    let path = file_path(&app).ok_or("no config folder")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&preferences).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    *store.0.lock().unwrap() = preferences.clone();
    if let Err(err) = app.emit(EVENT_CHANGED, &preferences) {
        log::warn!("failed to announce the new preferences: {err}");
    }
    Ok(preferences)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_older_and_newer_files() {
        let empty: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Preferences::default());
        assert_eq!(empty.auto_hide_delay_ms, 1000);

        let future: Preferences =
            serde_json::from_str(r#"{"islandVisibility":"autoHide","somethingNew":true}"#).unwrap();
        assert_eq!(future.island_visibility, IslandVisibility::AutoHide);

        let hidden = Preferences { island_visibility: IslandVisibility::Hidden, auto_hide_delay_ms: 1500 };
        assert_eq!(
            serde_json::to_string(&hidden).unwrap(),
            r#"{"islandVisibility":"hidden","autoHideDelayMs":1500}"#
        );
    }

    #[test]
    fn keeps_the_delay_in_range() {
        let too_short = Preferences { auto_hide_delay_ms: 0, ..Preferences::default() };
        let too_long = Preferences { auto_hide_delay_ms: 600_000, ..Preferences::default() };
        assert_eq!(too_short.clamped().auto_hide_delay_ms, 200);
        assert_eq!(too_long.clamped().auto_hide_delay_ms, 10_000);
    }
}
