//! User preferences, stored as JSON in the app config folder. Every change is broadcast to all
//! windows, so the island reacts to the settings window immediately.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT_CHANGED: &str = "preferences://changed";
const FILE_NAME: &str = "preferences.json";
/// Most items of a list in the file tried one by one when the list as a whole cannot be read.
const MAX_LIST_ITEMS: usize = 4 * MAX_WIDGET_SLOTS;

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
/// The island shows its widgets in up to this many rows.
pub const WIDGET_ROWS: u8 = 2;

/// A widget's place in the island and whether it is shown. Built-in widgets use ids such as
/// `builtin:media`, which an installed widget cannot take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSlot {
    pub id: String,
    pub enabled: bool,
    /// Row of the island the widget is in, counted from 0; files without it put every widget
    /// in the first row.
    #[serde(default)]
    pub row: u8,
}

/// Unknown fields are ignored and missing ones take their defaults, so files written by other
/// versions still load. A file is read value by value (see [`parse`]), so a value this version
/// cannot read takes its default without losing the others.
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
    /// Order of the widgets in the island; each row shows its widgets in this order. Widgets
    /// missing from the list are shown in the first row after the listed ones; ids of widgets
    /// that are not installed right now are kept, so a widget that comes back takes its old place.
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
    /// Global shortcuts for the island and the music.
    pub hotkeys: crate::hotkeys::Hotkeys,
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
            hotkeys: crate::hotkeys::Hotkeys::default(),
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
        self.hotkeys = self.hotkeys.cleaned();
        for slot in &mut self.widgets {
            slot.row = slot.row.min(WIDGET_ROWS - 1);
        }
        self
    }
}

pub struct Store(Mutex<Preferences>);

pub fn init(app: &AppHandle) {
    let preferences = file_path(app).map(|path| load(&path)).unwrap_or_default();
    app.manage(Store(Mutex::new(preferences)));
}

/// Reads the preferences file. When some of it cannot be read, the file is first copied to
/// `preferences.json.bak`, since the next save leaves out what could not be read.
fn load(path: &Path) -> Preferences {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            if err.kind() != ErrorKind::NotFound {
                log::warn!("cannot read {FILE_NAME}: {err}");
            }
            return Preferences::default();
        }
    };
    let (preferences, unreadable) = parse(&String::from_utf8_lossy(&bytes));
    if !unreadable.is_empty() {
        log::warn!("{FILE_NAME}: {} could not be read and took the default", unreadable.join(", "));
        if let Err(err) = fs::write(path.with_extension("json.bak"), &bytes) {
            log::warn!("could not keep a copy of {FILE_NAME}: {err}");
        }
    }
    preferences
}

/// The preferences in `text` and the names of the values that could not be read. Each value is
/// taken from the file only if the preferences can still be read with it, so a value this
/// version does not know, such as a new choice, or a number out of range for its type, takes
/// its default and the rest of the file is kept. Objects are read value by value and lists
/// item by item.
fn parse(text: &str) -> (Preferences, Vec<String>) {
    let Ok(mut merged) = serde_json::to_value(Preferences::default()) else {
        return (Preferences::default(), Vec::new());
    };
    let file = match serde_json::from_str::<Value>(text) {
        Ok(file) if file.is_object() => file,
        Ok(_) => return (Preferences::default(), vec!["the whole file".into()]),
        Err(err) => return (Preferences::default(), vec![format!("the whole file ({err})")]),
    };
    let mut unreadable = Vec::new();
    merge(&mut merged, "", &file, &mut unreadable);
    let preferences = Preferences::deserialize(&merged).map(Preferences::clamped).unwrap_or_default();
    (preferences, unreadable)
}

/// Copies the values of the object `file` into `merged` at the JSON pointer `at`.
fn merge(merged: &mut Value, at: &str, file: &Value, unreadable: &mut Vec<String>) {
    let readable = |value: &Value| Preferences::deserialize(value).is_ok();
    let Value::Object(fields) = file else { return };
    for (key, value) in fields {
        let pointer = format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
        // Fields this version does not know are left out, as before.
        let Some(current) = merged.pointer(&pointer) else { continue };
        if current.is_object() && value.is_object() {
            merge(merged, &pointer, value, unreadable);
            continue;
        }
        let with = |merged: &Value, value: Value| {
            let mut trial = merged.clone();
            if let Some(slot) = trial.pointer_mut(&pointer) {
                *slot = value;
            }
            trial
        };
        let trial = with(merged, value.clone());
        if readable(&trial) {
            *merged = trial;
            continue;
        }
        let name = pointer.trim_start_matches('/').replace('/', ".");
        let Value::Array(items) = value else {
            unreadable.push(name);
            continue;
        };
        let mut kept = with(merged, Value::Array(Vec::new()));
        if !readable(&kept) {
            unreadable.push(name);
            continue;
        }
        for item in items.iter().take(MAX_LIST_ITEMS) {
            let mut trial = kept.clone();
            if let Some(Value::Array(list)) = trial.pointer_mut(&pointer) {
                list.push(item.clone());
            }
            if readable(&trial) {
                kept = trial;
            }
        }
        *merged = kept;
        unreadable.push(format!("some of {name}"));
    }
}

/// Writes the file next to the old one and swaps it in, so a crash never leaves half a file.
fn save(path: &Path, preferences: &Preferences) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(preferences).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("json.tmp");
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(json.as_bytes())?;
        file.sync_all()
    });
    written.and_then(|()| fs::rename(&temporary, path)).map_err(|e| e.to_string())
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
    save(&path, &preferences)?;
    *store.0.lock().unwrap() = preferences.clone();
    crate::browser::configure(&app, preferences.browser_media, preferences.browser_media_port);
    crate::i18n::apply(&app, &preferences.language);
    crate::clipboard::configure(&app, &preferences);
    crate::hotkeys::configure(&app, &preferences.hotkeys);
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
            widgets: vec![WidgetSlot { id: "builtin:media".into(), enabled: false, row: 1 }],
            clipboard_history: false,
            clipboard_text: true,
            clipboard_pictures: false,
            clipboard_files: true,
            clipboard_lifetime_hours: 0,
            clipboard_limit_enabled: false,
            clipboard_limit: 200,
            hotkeys: crate::hotkeys::Hotkeys { toggle_island: None, ..Default::default() },
        };
        assert_eq!(
            serde_json::to_string(&hidden).unwrap(),
            r#"{"language":"ru","islandVisibility":"hidden","autoHideDelayMs":1500,"showOnMediaChange":false,"openDelayMs":400,"browserMedia":false,"browserMediaPort":7000,"widgets":[{"id":"builtin:media","enabled":false,"row":1}],"clipboardHistory":false,"clipboardText":true,"clipboardPictures":false,"clipboardFiles":true,"clipboardLifetimeHours":0,"clipboardLimitEnabled":false,"clipboardLimit":200,"hotkeys":{"toggleIsland":null,"playPause":null,"nextTrack":null,"previousTrack":null}}"#
        );
    }

    #[test]
    fn keeps_what_it_can_read_of_a_file() {
        let text = r#"{
            "islandVisibility": "pinned",
            "autoHideDelayMs": 5000000000,
            "openDelayMs": 300,
            "language": "ru",
            "browserMediaPort": "six",
            "clipboardLimit": 120,
            "widgets": [{"id": "todo", "enabled": false, "row": 1}, {"id": 5}, {"id": "notes", "enabled": true}],
            "hotkeys": {"toggleIsland": "Ctrl+Alt+F13", "playPause": 7},
            "somethingNew": {"x": 1}
        }"#;
        let (preferences, mut unreadable) = parse(text);
        unreadable.sort();
        assert_eq!(preferences.island_visibility, IslandVisibility::Always);
        assert_eq!(preferences.auto_hide_delay_ms, 1000);
        assert_eq!(preferences.browser_media_port, crate::browser::DEFAULT_PORT);
        assert_eq!((preferences.open_delay_ms, preferences.language.as_str(), preferences.clipboard_limit), (300, "ru", 120));
        let slot = |id: &str, enabled, row| WidgetSlot { id: id.into(), enabled, row };
        assert_eq!(preferences.widgets, vec![slot("todo", false, 1), slot("notes", true, 0)]);
        assert_eq!(preferences.hotkeys.toggle_island.as_deref(), Some("Ctrl+Alt+F13"));
        assert_eq!(preferences.hotkeys.play_pause, None);
        assert_eq!(
            unreadable,
            ["autoHideDelayMs", "browserMediaPort", "hotkeys.playPause", "islandVisibility", "some of widgets"]
        );

        let (fine, unreadable) = parse(&serde_json::to_string(&preferences).unwrap());
        assert_eq!((fine, unreadable.len()), (preferences, 0));
        for broken in ["", "{\"language\": \"ru\"", "[1, 2]", "null"] {
            let (preferences, unreadable) = parse(broken);
            assert_eq!(preferences, Preferences::default());
            assert_eq!(unreadable.len(), 1, "{broken}");
        }
    }

    #[test]
    fn saves_whole_files_and_keeps_unreadable_ones() {
        let dir = std::env::temp_dir().join(format!("lizun-preferences-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join(FILE_NAME);
        let backup = dir.join("preferences.json.bak");
        assert_eq!(load(&path), Preferences::default());

        let russian = Preferences { language: "ru".into(), clipboard_limit: 75, ..Preferences::default() };
        save(&path, &russian).unwrap();
        assert_eq!(load(&path), russian);
        assert!(!dir.join("preferences.json.tmp").exists() && !backup.exists());
        save(&path, &Preferences::default()).unwrap();
        assert_eq!(load(&path), Preferences::default());

        let newer = r#"{"language":"ru","islandVisibility":"pinned"}"#;
        fs::write(&path, newer).unwrap();
        assert_eq!(load(&path).language, "ru");
        assert_eq!(fs::read_to_string(&backup).unwrap(), newer);
        let _ = fs::remove_dir_all(&dir);
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
        let slot = |id: &str, enabled| WidgetSlot { id: id.into(), enabled, row: 0 };
        let messy = Preferences {
            widgets: vec![
                slot("builtin:calendar", true),
                slot("", true),
                slot("builtin:calendar", false),
                slot(&"x".repeat(MAX_WIDGET_ID_LEN + 1), true),
                WidgetSlot { row: 7, ..slot("todo", false) },
            ],
            ..Preferences::default()
        };
        assert_eq!(
            messy.clamped().widgets,
            vec![slot("builtin:calendar", true), WidgetSlot { row: WIDGET_ROWS - 1, ..slot("todo", false) }]
        );

        let older: Preferences = serde_json::from_str(r#"{"widgets":[{"id":"todo","enabled":true}]}"#).unwrap();
        assert_eq!(older.widgets, vec![slot("todo", true)]);

        let many = Preferences {
            widgets: (0..MAX_WIDGET_SLOTS + 10).map(|i| slot(&format!("widget-{i}"), true)).collect(),
            ..Preferences::default()
        };
        assert_eq!(many.clamped().widgets.len(), MAX_WIDGET_SLOTS);
    }
}
