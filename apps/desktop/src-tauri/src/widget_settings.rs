//! Per-widget settings: the options a widget declares in its manifest and the values the user
//! picks in the settings window (see docs/widgets.md#settings and ADR 0002).
//!
//! Lizun owns the values. Every write is checked against the widget's schema, and stored values
//! are fixed up against the current schema whenever they are read, so a widget always receives
//! every declared key with a value of the declared type. Widgets can only read them.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

/// Sent to every window with `{ widgetId, values }` after a widget's values change.
pub const EVENT_CHANGED: &str = "widget-settings://changed";
const FILE_NAME: &str = "widget-settings.json";
const MAX_SETTINGS: usize = 32;

/// Values of one widget, by setting key.
pub type Values = serde_json::Map<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// The kind of file a text setting can be filled with from a file dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChoice {
    /// Shown in the dialog's file type list, such as "Calendar".
    pub name: String,
    /// Extensions without the dot, such as `ics`.
    pub extensions: Vec<String>,
}

/// One entry of the manifest's `settings` list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum SettingSpec {
    Toggle {
        key: String,
        label: String,
        #[serde(default)]
        description: String,
        default: bool,
    },
    Number {
        key: String,
        label: String,
        #[serde(default)]
        description: String,
        default: f64,
        min: f64,
        max: f64,
        #[serde(default = "default_step")]
        step: f64,
        #[serde(default)]
        unit: String,
    },
    Select {
        key: String,
        label: String,
        #[serde(default)]
        description: String,
        default: String,
        options: Vec<SelectOption>,
    },
    Text {
        key: String,
        label: String,
        #[serde(default)]
        description: String,
        default: String,
        #[serde(default = "default_max_length")]
        max_length: usize,
        #[serde(default)]
        placeholder: String,
        /// Adds a button that fills the field with the path of a file picked in a dialog. Only
        /// for built-in widgets: an installed widget runs in a sandbox and cannot read files, and
        /// the path would tell it about the user's folders.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<FileChoice>,
    },
}

fn default_step() -> f64 {
    1.0
}

fn default_max_length() -> usize {
    100
}

impl SettingSpec {
    pub fn key(&self) -> &str {
        match self {
            Self::Toggle { key, .. } | Self::Number { key, .. } | Self::Select { key, .. } | Self::Text { key, .. } => key,
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Toggle { label, .. }
            | Self::Number { label, .. }
            | Self::Select { label, .. }
            | Self::Text { label, .. } => label,
        }
    }

    fn description(&self) -> &str {
        match self {
            Self::Toggle { description, .. }
            | Self::Number { description, .. }
            | Self::Select { description, .. }
            | Self::Text { description, .. } => description,
        }
    }

    /// True for a text setting that can be filled from a file dialog.
    pub fn picks_file(&self) -> bool {
        matches!(self, Self::Text { file: Some(_), .. })
    }

    fn default_value(&self) -> Value {
        match self {
            Self::Toggle { default, .. } => Value::Bool(*default),
            Self::Number { default, .. } => number(*default),
            Self::Select { default, .. } | Self::Text { default, .. } => Value::String(default.clone()),
        }
    }

    /// Brings a value of the right type into range: numbers are clamped, text is cut to
    /// `maxLength`. Returns `None` for a value of another type or an option that does not exist.
    fn coerce(&self, value: &Value) -> Option<Value> {
        match self {
            Self::Toggle { .. } => value.as_bool().map(Value::Bool),
            Self::Number { min, max, .. } => {
                value.as_f64().filter(|v| v.is_finite()).map(|v| number(v.clamp(*min, *max)))
            }
            Self::Select { options, .. } => {
                let value = value.as_str()?;
                options.iter().any(|o| o.value == value).then(|| Value::String(value.into()))
            }
            Self::Text { max_length, .. } => {
                value.as_str().map(|text| Value::String(text.chars().take(*max_length).collect()))
            }
        }
    }
}

/// Whole numbers are stored as integers, so `15` does not come back as `15.0`.
fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9e15 {
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

/// Checks the manifest's `settings` list; the error says which setting is wrong.
pub fn validate(settings: &[SettingSpec]) -> Result<(), String> {
    if settings.len() > MAX_SETTINGS {
        return Err(format!("more than {MAX_SETTINGS} settings"));
    }
    let mut keys = HashSet::new();
    for setting in settings {
        let key = setting.key();
        let key_ok = key.len() <= 32
            && key.starts_with(|c: char| c.is_ascii_alphabetic())
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !key_ok {
            return Err(format!("invalid setting key {key:?}: use a letter, then up to 31 letters, digits or _"));
        }
        if !keys.insert(key) {
            return Err(format!("setting key {key:?} is used twice"));
        }
        let field = |name: &str| format!("setting {key:?}: {name}");
        check_length(&field("label"), setting.label(), 1..=60)?;
        check_length(&field("description"), setting.description(), 0..=200)?;
        match setting {
            SettingSpec::Toggle { .. } => {}
            SettingSpec::Number { default, min, max, step, unit, .. } => {
                if ![default, min, max, step].iter().all(|v| v.is_finite()) || min > max || *step <= 0.0 {
                    return Err(field("needs finite numbers, min <= max and a positive step"));
                }
                if default < min || default > max {
                    return Err(field("default is outside min..=max"));
                }
                check_length(&field("unit"), unit, 0..=10)?;
            }
            SettingSpec::Select { default, options, .. } => {
                if !(2..=20).contains(&options.len()) {
                    return Err(field("needs 2 to 20 options"));
                }
                let mut values = HashSet::new();
                for option in options {
                    check_length(&field("option value"), &option.value, 1..=64)?;
                    check_length(&field("option label"), &option.label, 1..=60)?;
                    if !values.insert(option.value.as_str()) {
                        return Err(field(&format!("option {:?} is listed twice", option.value)));
                    }
                }
                if !values.contains(default.as_str()) {
                    return Err(field("default is not one of the options"));
                }
            }
            SettingSpec::Text { default, max_length, placeholder, file, .. } => {
                if !(1..=500).contains(max_length) {
                    return Err(field("maxLength must be 1 to 500"));
                }
                check_length(&field("default"), default, 0..=*max_length)?;
                check_length(&field("placeholder"), placeholder, 0..=100)?;
                if let Some(file) = file {
                    check_length(&field("file name"), &file.name, 1..=60)?;
                    let extension_ok = |e: &String| (1..=10).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric());
                    if !(1..=10).contains(&file.extensions.len()) || !file.extensions.iter().all(extension_ok) {
                        return Err(field("file needs 1 to 10 extensions of letters and digits, without the dot"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_length(what: &str, text: &str, allowed: RangeInclusive<usize>) -> Result<(), String> {
    let length = text.chars().count();
    if allowed.contains(&length) {
        Ok(())
    } else {
        Err(format!("{what} must be {} to {} characters long", allowed.start(), allowed.end()))
    }
}

/// Every declared key with a valid value: the stored one, fixed up, or the default. Stored keys
/// that are no longer declared are left out.
pub fn resolve(settings: &[SettingSpec], stored: Option<&Values>) -> Values {
    settings
        .iter()
        .map(|setting| {
            let value = stored.and_then(|values| values.get(setting.key())).and_then(|v| setting.coerce(v));
            (setting.key().to_string(), value.unwrap_or_else(|| setting.default_value()))
        })
        .collect()
}

/// Stored values by widget id. Values of uninstalled widgets are kept, so reinstalling a widget
/// restores them.
pub struct Store(Mutex<HashMap<String, Values>>);

pub fn init(app: &AppHandle) {
    let stored = file_path(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| {
            serde_json::from_str(&text).unwrap_or_else(|err| {
                log::warn!("ignoring unreadable {FILE_NAME}: {err}");
                HashMap::new()
            })
        })
        .unwrap_or_default();
    app.manage(Store(Mutex::new(stored)));
}

fn file_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join(FILE_NAME))
}

fn save(app: &AppHandle, stored: &HashMap<String, Values>) -> Result<(), String> {
    let path = file_path(app).ok_or("no config folder")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(stored).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Changed<'a> {
    widget_id: &'a str,
    values: &'a Values,
}

/// Replaces a widget's stored values, saves them and tells every window.
fn commit(app: &AppHandle, store: &Store, widget_id: &str, stored: Option<Values>, values: &Values) -> Result<(), String> {
    let mut all = store.0.lock().unwrap();
    let mut next = all.clone();
    match stored {
        Some(stored) => next.insert(widget_id.to_string(), stored),
        None => next.remove(widget_id),
    };
    save(app, &next)?;
    *all = next;
    drop(all);
    if let Err(err) = app.emit(EVENT_CHANGED, Changed { widget_id, values }) {
        log::warn!("failed to announce new settings of {widget_id}: {err}");
    }
    if widget_id == crate::calendar::WIDGET_ID {
        crate::calendar::settings_changed(app);
    }
    Ok(())
}

/// Settings of the widgets that are part of the app, declared here instead of in a manifest.
fn built_in() -> Vec<(&'static str, Vec<SettingSpec>)> {
    vec![(crate::calendar::WIDGET_ID, crate::calendar::settings())]
}

fn settings_of(app: &AppHandle, widget_id: &str) -> Result<Vec<SettingSpec>, String> {
    if let Some((_, settings)) = built_in().into_iter().find(|(id, _)| *id == widget_id) {
        return Ok(settings);
    }
    crate::widgets::find(app, widget_id)
        .map(|manifest| manifest.settings)
        .ok_or_else(|| format!("no installed widget {widget_id:?}"))
}

/// Current values of a widget, with defaults for anything the user has not set.
pub fn values_of(app: &AppHandle, widget_id: &str) -> Values {
    let Ok(settings) = settings_of(app, widget_id) else { return Values::new() };
    resolve(&settings, app.state::<Store>().0.lock().unwrap().get(widget_id))
}

/// Settings of the built-in widgets that have any, by widget id.
#[tauri::command]
pub fn widget_settings_built_in() -> HashMap<&'static str, Vec<SettingSpec>> {
    built_in().into_iter().collect()
}

/// Values of every built-in or installed widget that declares settings.
#[tauri::command]
pub fn widget_settings_all(app: AppHandle, store: State<'_, Store>) -> HashMap<String, Values> {
    let stored = store.0.lock().unwrap();
    let built_in = built_in().into_iter().map(|(id, settings)| (id.to_string(), settings));
    let installed = crate::widgets::installed(&app).into_iter().map(|manifest| (manifest.id, manifest.settings));
    built_in
        .chain(installed)
        .filter(|(_, settings)| !settings.is_empty())
        .map(|(id, settings)| {
            let values = resolve(&settings, stored.get(&id));
            (id, values)
        })
        .collect()
}

/// Changes one value. Numbers are clamped and text is cut to fit; a value of the wrong type or an
/// unknown option is refused.
#[tauri::command]
pub fn widget_settings_set(
    app: AppHandle,
    store: State<'_, Store>,
    widget_id: String,
    key: String,
    value: Value,
) -> Result<Values, String> {
    let settings = settings_of(&app, &widget_id)?;
    let setting = settings
        .iter()
        .find(|s| s.key() == key)
        .ok_or_else(|| format!("{widget_id} has no setting {key:?}"))?;
    let value = setting.coerce(&value).ok_or_else(|| format!("{value} is not a valid value for {key:?}"))?;
    let mut values = resolve(&settings, store.0.lock().unwrap().get(&widget_id));
    values.insert(key, value);
    commit(&app, &store, &widget_id, Some(values.clone()), &values)?;
    Ok(values)
}

/// Forgets a widget's values, so every setting is back at its default.
#[tauri::command]
pub fn widget_settings_reset(app: AppHandle, store: State<'_, Store>, widget_id: String) -> Result<Values, String> {
    let values = resolve(&settings_of(&app, &widget_id)?, None);
    commit(&app, &store, &widget_id, None, &values)?;
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(value: Value) -> Result<Vec<SettingSpec>, String> {
        let settings: Vec<SettingSpec> = serde_json::from_value(value).map_err(|e| e.to_string())?;
        validate(&settings)?;
        Ok(settings)
    }

    fn weather() -> Vec<SettingSpec> {
        parse(json!([
            { "key": "city", "type": "text", "label": "City", "default": "Moscow", "maxLength": 10 },
            {
                "key": "units", "type": "select", "label": "Units", "default": "metric",
                "options": [{ "value": "metric", "label": "Celsius" }, { "value": "imperial", "label": "Fahrenheit" }]
            },
            { "key": "refresh", "type": "number", "label": "Refresh every", "unit": "min", "default": 15, "min": 5, "max": 120 },
            { "key": "feelsLike", "type": "toggle", "label": "Feels like", "default": true }
        ]))
        .unwrap()
    }

    #[test]
    fn reads_the_documented_format() {
        let settings = weather();
        assert_eq!(settings.len(), 4);
        assert!(matches!(&settings[2], SettingSpec::Number { step, unit, .. } if *step == 1.0 && unit == "min"));
        assert!(matches!(&settings[0], SettingSpec::Text { placeholder, .. } if placeholder.is_empty()));
    }

    #[test]
    fn rejects_broken_schemas() {
        let bad = [
            json!([{ "key": "a", "type": "toggle", "label": "A" }]),
            json!([{ "key": "a", "type": "toggle", "label": "A", "default": "yes" }]),
            json!([{ "key": "a", "type": "slider", "label": "A", "default": 1 }]),
            json!([{ "key": "a", "type": "toggle", "label": "A", "default": true, "colour": "red" }]),
            json!([{ "key": "1a", "type": "toggle", "label": "A", "default": true }]),
            json!([{ "key": "a-b", "type": "toggle", "label": "A", "default": true }]),
            json!([{ "key": "a", "type": "toggle", "label": "", "default": true }]),
            json!([
                { "key": "a", "type": "toggle", "label": "A", "default": true },
                { "key": "a", "type": "toggle", "label": "B", "default": false }
            ]),
            json!([{ "key": "n", "type": "number", "label": "N", "default": 1 }]),
            json!([{ "key": "n", "type": "number", "label": "N", "default": 50, "min": 0, "max": 10 }]),
            json!([{ "key": "n", "type": "number", "label": "N", "default": 5, "min": 10, "max": 0 }]),
            json!([{ "key": "n", "type": "number", "label": "N", "default": 5, "min": 0, "max": 10, "step": 0 }]),
            json!([{ "key": "s", "type": "select", "label": "S", "default": "x", "options": [{ "value": "x", "label": "X" }] }]),
            json!([{
                "key": "s", "type": "select", "label": "S", "default": "z",
                "options": [{ "value": "x", "label": "X" }, { "value": "y", "label": "Y" }]
            }]),
            json!([{
                "key": "s", "type": "select", "label": "S", "default": "x",
                "options": [{ "value": "x", "label": "X" }, { "value": "x", "label": "Y" }]
            }]),
            json!([{ "key": "t", "type": "text", "label": "T", "default": "too long", "maxLength": 3 }]),
            json!([{ "key": "t", "type": "text", "label": "T", "default": "", "maxLength": 501 }]),
        ];
        for schema in bad {
            assert!(parse(schema.clone()).is_err(), "{schema} should be rejected");
        }
        let many: Vec<Value> = (0..33).map(|i| json!({ "key": format!("k{i}"), "type": "toggle", "label": "K", "default": true })).collect();
        assert!(parse(Value::Array(many)).is_err());
    }

    #[test]
    fn fills_in_defaults_and_fixes_stored_values() {
        let settings = weather();
        let defaults = resolve(&settings, None);
        assert_eq!(Value::Object(defaults), json!({ "city": "Moscow", "units": "metric", "refresh": 15, "feelsLike": true }));

        let stored = json!({
            "city": "Saint Petersburg",
            "units": "kelvin",
            "refresh": 1000,
            "feelsLike": "no",
            "removed": 1
        });
        let fixed = resolve(&settings, stored.as_object());
        assert_eq!(Value::Object(fixed), json!({ "city": "Saint Pete", "units": "metric", "refresh": 120, "feelsLike": true }));
    }

    #[test]
    fn coerces_written_values() {
        let settings = weather();
        let refresh = &settings[2];
        assert_eq!(refresh.coerce(&json!(2)), Some(json!(5)));
        assert_eq!(refresh.coerce(&json!(7.5)), Some(json!(7.5)));
        assert_eq!(refresh.coerce(&json!("7")), None);
        assert_eq!(settings[1].coerce(&json!("imperial")), Some(json!("imperial")));
        assert_eq!(settings[1].coerce(&json!("kelvin")), None);
        assert_eq!(settings[3].coerce(&json!(1)), None);
    }
}
