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
/// Allowed range for how long clipboard history entries stay, in hours; 0 keeps them for good.
pub const CLIPBOARD_LIFETIME_HOURS: std::ops::RangeInclusive<u32> = 0..=8_760;
/// Allowed range for the number of clipboard history entries, when it is limited.
pub const CLIPBOARD_LIMIT: std::ops::RangeInclusive<u32> = 1..=1_000;
/// Limits for the widget list, which only ever holds ids the user has seen in the island.
const MAX_WIDGET_SLOTS: usize = 256;
const MAX_WIDGET_ID_LEN: usize = 80;

/// A widget's place in the island and whether it is shown. Built-in widgets use ids such as
/// `builtin:media`, which an installed widget cannot take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSlot {
    pub id: String,
    pub enabled: bool,
}

/// Unknown fields are ignored and missing ones take their defaults, so files written by other
/// versions still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// Interface language: a code such as `ru`, or `system` to follow Windows.
    pub language: String,
    pub island_visibility: IslandVisibility,
    pub auto_hide_delay_ms: u32,
    /// In auto-hide mode, show the mini island when a track starts, pauses or changes.
    pub show_on_media_change: bool,
    /// How long the cursor has to stay on the island (or the top edge while it is hidden)
    /// before it opens, so passing over it does not open it by accident.
    pub open_delay_ms: u32,
    /// Accept media from the Lizun browser extension.
    pub browser_media: bool,
    /// Port the extension connects to; its popup must show the same one.
    pub browser_media_port: u16,
    /// Order of the widgets in the island. Widgets missing from the list are shown after the
    /// listed ones; ids of widgets that are not installed right now are kept, so a widget
    /// that comes back takes its old place.
    pub widgets: Vec<WidgetSlot>,
    /// Keep a history of copied text, pictures and files for the island's Clipboard tab.
    pub clipboard_history: bool,
    /// Which kinds of clips the history keeps.
    pub clipboard_text: bool,
    pub clipboard_pictures: bool,
    pub clipboard_files: bool,
    /// How long history entries stay, in hours; 0 keeps them until the limit pushes them out.
    pub clipboard_lifetime_hours: u32,
    /// Whether the history keeps at most `clipboard_limit` entries.
    pub clipboard_limit_enabled: bool,
    pub clipboard_limit: u32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: crate::i18n::SYSTEM.into(),
            island_visibility: IslandVisibility::default(),
            auto_hide_delay_ms: 1000,
            show_on_media_change: true,
            open_delay_ms: 100,
            browser_media: true,
            browser_media_port: crate::browser::DEFAULT_PORT,
            widgets: Vec::new(),
            clipboard_history: true,
            clipboard_text: true,
            clipboard_pictures: true,
            clipboard_files: true,
            clipboard_lifetime_hours: 24,
            clipboard_limit_enabled: true,
            clipboard_limit: 50,
        }
    }
}

impl Preferences {
    fn clamped(mut self) -> Self {
        if !crate::i18n::is_supported(&self.language) {
            self.language = crate::i18n::SYSTEM.into();
        }
        self.auto_hide_delay_ms =
            self.auto_hide_delay_ms.clamp(*AUTO_HIDE_DELAY_MS.start(), *AUTO_HIDE_DELAY_MS.end());
        self.open_delay_ms = self.open_delay_ms.clamp(*OPEN_DELAY_MS.start(), *OPEN_DELAY_MS.end());
        self.browser_media_port =
            self.browser_media_port.clamp(*BROWSER_MEDIA_PORTS.start(), *BROWSER_MEDIA_PORTS.end());
        self.clipboard_lifetime_hours = self.clipboard_lifetime_hours.min(*CLIPBOARD_LIFETIME_HOURS.end());
        self.clipboard_limit = self.clipboard_limit.clamp(*CLIPBOARD_LIMIT.start(), *CLIPBOARD_LIMIT.end());
        let mut seen = std::collections::HashSet::new();
        self.widgets.retain(|slot| {
            !slot.id.is_empty() && slot.id.len() <= MAX_WIDGET_ID_LEN && seen.insert(slot.id.clone())
        });
        self.widgets.truncate(MAX_WIDGET_SLOTS);
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
    crate::i18n::apply(&app, &preferences.language);
    crate::clipboard::configure(&app, &preferences);
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
        assert!(empty.show_on_media_change);
        assert_eq!(empty.language, "system");

        let future: Preferences =
            serde_json::from_str(r#"{"islandVisibility":"autoHide","somethingNew":true}"#).unwrap();
        assert_eq!(future.island_visibility, IslandVisibility::AutoHide);
        assert!(future.browser_media);
        assert_eq!(future.browser_media_port, 6869);

        let hidden = Preferences {
            language: "ru".into(),
            island_visibility: IslandVisibility::Hidden,
            auto_hide_delay_ms: 1500,
            show_on_media_change: false,
            open_delay_ms: 400,
            browser_media: false,
            browser_media_port: 7000,
            widgets: vec![WidgetSlot { id: "builtin:media".into(), enabled: false }],
            clipboard_history: false,
            clipboard_text: true,
            clipboard_pictures: false,
            clipboard_files: true,
            clipboard_lifetime_hours: 0,
            clipboard_limit_enabled: false,
            clipboard_limit: 200,
        };
        assert_eq!(
            serde_json::to_string(&hidden).unwrap(),
            r#"{"language":"ru","islandVisibility":"hidden","autoHideDelayMs":1500,"showOnMediaChange":false,"openDelayMs":400,"browserMedia":false,"browserMediaPort":7000,"widgets":[{"id":"builtin:media","enabled":false}],"clipboardHistory":false,"clipboardText":true,"clipboardPictures":false,"clipboardFiles":true,"clipboardLifetimeHours":0,"clipboardLimitEnabled":false,"clipboardLimit":200}"#
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
    fn forgets_languages_it_has_no_translation_for() {
        let gone = Preferences { language: "xx".into(), ..Preferences::default() };
        assert_eq!(gone.clamped().language, "system");
        let russian = Preferences { language: "ru".into(), ..Preferences::default() };
        assert_eq!(russian.clamped().language, "ru");
    }

    #[test]
    fn keeps_the_browser_media_port_out_of_system_ports() {
        let system_port = Preferences { browser_media_port: 80, ..Preferences::default() };
        assert_eq!(system_port.clamped().browser_media_port, 1024);
    }

    #[test]
    fn keeps_the_clipboard_history_settings_in_range() {
        let empty: Preferences = serde_json::from_str("{}").unwrap();
        assert!(empty.clipboard_history && empty.clipboard_limit_enabled);
        assert!(empty.clipboard_text && empty.clipboard_pictures && empty.clipboard_files);
        assert_eq!((empty.clipboard_lifetime_hours, empty.clipboard_limit), (24, 50));
        let wild = Preferences { clipboard_lifetime_hours: 100_000, clipboard_limit: 0, ..Preferences::default() }.clamped();
        assert_eq!((wild.clipboard_lifetime_hours, wild.clipboard_limit), (8_760, 1));
    }

    #[test]
    fn cleans_up_the_widget_list() {
        let slot = |id: &str, enabled| WidgetSlot { id: id.into(), enabled };
        let messy = Preferences {
            widgets: vec![
                slot("builtin:calendar", true),
                slot("", true),
                slot("builtin:calendar", false),
                slot(&"x".repeat(MAX_WIDGET_ID_LEN + 1), true),
                slot("todo", false),
            ],
            ..Preferences::default()
        };
        assert_eq!(messy.clamped().widgets, vec![slot("builtin:calendar", true), slot("todo", false)]);

        let many = Preferences {
            widgets: (0..MAX_WIDGET_SLOTS + 10).map(|i| slot(&format!("widget-{i}"), true)).collect(),
            ..Preferences::default()
        };
        assert_eq!(many.clamped().widgets.len(), MAX_WIDGET_SLOTS);
    }
}
