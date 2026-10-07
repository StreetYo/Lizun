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
/// Allowed range for how long the cursor has to stay on the island before it opens.
pub const OPEN_DELAY_MS: std::ops::RangeInclusive<u32> = 0..=2_000;
/// Ports below 1024 are for system services.
pub const BROWSER_MEDIA_PORTS: std::ops::RangeInclusive<u16> = 1024..=65_535;

/// Unknown fields are ignored and missing ones take their defaults, so files written by other
/// versions still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub island_visibility: IslandVisibility,
    pub auto_hide_delay_ms: u32,
    /// How long the cursor has to stay on the island (or the top edge while it is hidden)
    /// before it opens, so passing over it does not open it by accident.
    pub open_delay_ms: u32,
    /// Accept media from the Lizun browser extension.
    pub browser_media: bool,
    /// Port the extension connects to; its popup must show the same one.
    pub browser_media_port: u16,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            island_visibility: IslandVisibility::default(),
            auto_hide_delay_ms: 1000,
            open_delay_ms: 100,
            browser_media: true,
            browser_media_port: crate::browser::DEFAULT_PORT,
        }
    }
}

impl Preferences {
    fn clamped(mut self) -> Self {
        self.auto_hide_delay_ms =
            self.auto_hide_delay_ms.clamp(*AUTO_HIDE_DELAY_MS.start(), *AUTO_HIDE_DELAY_MS.end());
        self.open_delay_ms = self.open_delay_ms.clamp(*OPEN_DELAY_MS.start(), *OPEN_DELAY_MS.end());
        self.browser_media_port =
            self.browser_media_port.clamp(*BROWSER_MEDIA_PORTS.start(), *BROWSER_MEDIA_PORTS.end());
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

pub fn current(app: &AppHandle) -> Preferences {
    app.state::<Store>().0.lock().unwrap().clone()
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
    crate::browser::configure(&app, preferences.browser_media, preferences.browser_media_port);
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
        assert_eq!(empty.open_delay_ms, 100);

        let future: Preferences =
            serde_json::from_str(r#"{"islandVisibility":"autoHide","somethingNew":true}"#).unwrap();
        assert_eq!(future.island_visibility, IslandVisibility::AutoHide);
        assert!(future.browser_media);
        assert_eq!(future.browser_media_port, 6869);

        let hidden = Preferences {
            island_visibility: IslandVisibility::Hidden,
            auto_hide_delay_ms: 1500,
            open_delay_ms: 400,
            browser_media: false,
            browser_media_port: 7000,
        };
        assert_eq!(
            serde_json::to_string(&hidden).unwrap(),
            r#"{"islandVisibility":"hidden","autoHideDelayMs":1500,"openDelayMs":400,"browserMedia":false,"browserMediaPort":7000}"#
        );
    }

    #[test]
    fn keeps_the_delays_in_range() {
        let too_short = Preferences { auto_hide_delay_ms: 0, open_delay_ms: 0, ..Preferences::default() }.clamped();
        let too_long =
            Preferences { auto_hide_delay_ms: 600_000, open_delay_ms: 600_000, ..Preferences::default() }.clamped();
        assert_eq!((too_short.auto_hide_delay_ms, too_short.open_delay_ms), (200, 0));
        assert_eq!((too_long.auto_hide_delay_ms, too_long.open_delay_ms), (10_000, 2_000));
    }

    #[test]
    fn keeps_the_browser_media_port_out_of_system_ports() {
        let system_port = Preferences { browser_media_port: 80, ..Preferences::default() };
        assert_eq!(system_port.clamped().browser_media_port, 1024);
    }
}
