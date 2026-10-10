//! Per-widget settings: the options a widget declares in its manifest and the values the user
//! picks in the settings window (see docs/widgets.md#settings and ADR 0002).
//!
//! Lizun owns the values. Every write is checked against the widget's schema, and stored values
//! are fixed up against the current schema whenever they are read, so a widget always receives
//! every declared key with a value of the declared type. Widgets can only read them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
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
    /// An address the user types, such as their Home Assistant server. The widget may reach its
    /// origin without listing it in `network`: the user chose it (see `user_origin`).
    Url {
        key: String,
        label: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        default: String,
        #[serde(default)]
        placeholder: String,
    },
    /// A token or password. The settings window does not show it, and it is stored encrypted for
    /// this Windows user (`platform::protect`); the widget gets it in plain text.
    Secret {
        key: String,
        label: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        placeholder: String,
    },
}

/// A setting's texts in another language, from the manifest's `locales`. Whatever is left out
/// stays as declared in `settings`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingTexts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Text settings only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Number settings only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Select settings only: option labels by option value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
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
            Self::Toggle { key, .. }
            | Self::Number { key, .. }
            | Self::Select { key, .. }
            | Self::Text { key, .. }
            | Self::Url { key, .. }
            | Self::Secret { key, .. } => key,
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Toggle { label, .. }
            | Self::Number { label, .. }
            | Self::Select { label, .. }
            | Self::Text { label, .. }
            | Self::Url { label, .. }
            | Self::Secret { label, .. } => label,
        }
    }

    fn description(&self) -> &str {
        match self {
            Self::Toggle { description, .. }
            | Self::Number { description, .. }
            | Self::Select { description, .. }
            | Self::Text { description, .. }
            | Self::Url { description, .. }
            | Self::Secret { description, .. } => description,
        }
    }

    /// Text, url and secret settings have one.
    fn placeholder(&self) -> Option<&str> {
        match self {
            Self::Text { placeholder, .. } | Self::Url { placeholder, .. } | Self::Secret { placeholder, .. } => {
                Some(placeholder)
            }
            _ => None,
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
            Self::Select { default, .. } | Self::Text { default, .. } | Self::Url { default, .. } => {
                Value::String(default.clone())
            }
            Self::Secret { .. } => Value::String(String::new()),
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
            // An address is kept the way it was understood, so the user sees what is reached.
            Self::Url { .. } => match value.as_str()?.trim() {
                "" => Some(Value::String(String::new())),
                text => normalize_url(text).map(Value::String),
            },
            Self::Secret { .. } => {
                value.as_str().filter(|text| text.len() <= MAX_SECRET).map(|text| Value::String(text.into()))
            }
        }
    }
}

/// Longest secret, in bytes.
const MAX_SECRET: usize = 4096;
/// Longest address, in characters.
const MAX_URL: usize = 2048;
/// How a secret is kept in the settings file: DPAPI-encrypted bytes in base64 under this key.
const SECRET_FIELD: &str = "$protected";

/// The address as a URL Lizun lets a widget reach, or `None`: `https://` (or `wss://`) anywhere,
/// plain `http://` (or `ws://`) only to this computer and the local network, where servers
/// often have no certificate. An address typed without a scheme is read as `https://`.
pub fn normalize_url(text: &str) -> Option<String> {
    if text.chars().count() > MAX_URL {
        return None;
    }
    let with_scheme = if text.contains("://") { text.to_string() } else { format!("https://{text}") };
    let url = tauri::Url::parse(&with_scheme).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let allowed = match url.scheme() {
        "https" | "wss" => true,
        "http" | "ws" => is_local(url.host_str()?),
        _ => false,
    };
    (allowed && url.host_str().is_some_and(|host| !host.is_empty())).then(|| url.to_string())
}

/// This computer, private and link-local addresses, the shared range mesh VPNs such as
/// Tailscale use, and names that only resolve on a local network (`nas`, `printer.local`).
/// `host` is as a parsed URL has it: IPv6 addresses in brackets, names in lowercase.
fn is_local(host: &str) -> bool {
    match ip_address(host) {
        Some(IpAddr::V4(ip)) => {
            let [a, b, ..] = ip.octets();
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || (a == 100 && (64..128).contains(&b))
        }
        Some(IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
        None => {
            let name = host.trim_end_matches('.');
            !name.contains('.')
                || [".local", ".lan", ".home", ".home.arpa", ".internal", ".localhost"].iter().any(|end| name.ends_with(end))
        }
    }
}

fn ip_address(host: &str) -> Option<IpAddr> {
    host.trim_start_matches('[').trim_end_matches(']').parse().ok()
}

/// The origin a `url` setting's value lets the widget reach, for the CSP and `network.request`:
/// `https://host[:port]` for secure addresses and `http://host[:port]` for plain ones; in a CSP
/// they also cover `wss://` and `ws://`. `None` when the value is empty or invalid, or names a
/// refused port on this computer.
pub fn user_origin(value: &str) -> Option<String> {
    let url = tauri::Url::parse(&normalize_url(value)?).ok()?;
    let secure = matches!(url.scheme(), "https" | "wss");
    let host = url.host_str()?;
    let loopback = ip_address(host).map_or(host == "localhost", |ip| ip.is_loopback());
    let port = url.port_or_known_default().unwrap_or(if secure { 443 } else { 80 });
    if loopback && crate::widgets::FORBIDDEN_LOOPBACK_PORTS.contains(&port) {
        return None;
    }
    let scheme = if secure { "https" } else { "http" };
    Some(match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

/// Encrypts a secret for the settings file.
fn seal(secret: &str) -> Result<Value, String> {
    use base64::Engine;
    let sealed = crate::platform::protect(secret.as_bytes())?;
    Ok(serde_json::json!({ SECRET_FIELD: base64::engine::general_purpose::STANDARD.encode(sealed) }))
}

/// The secret from the settings file; `None` when it cannot be decrypted, for example in a file
/// copied from another computer or another Windows user.
fn unseal(value: &Value) -> Option<String> {
    use base64::Engine;
    let encoded = value.get(SECRET_FIELD)?.as_str()?;
    let sealed = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    match crate::platform::unprotect(&sealed) {
        Ok(bytes) => String::from_utf8(bytes).ok(),
        Err(err) => {
            log::warn!("a widget secret cannot be decrypted: {err}");
            None
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
            SettingSpec::Url { default, placeholder, .. } => {
                if !default.is_empty() && normalize_url(default).as_deref() != Some(default.as_str()) {
                    return Err(field(
                        "default must be empty or a full https:// address (http:// only on the local network) as Lizun writes it, such as https://example.com/",
                    ));
                }
                check_length(&field("placeholder"), placeholder, 0..=100)?;
            }
            SettingSpec::Secret { placeholder, .. } => check_length(&field("placeholder"), placeholder, 0..=100)?,
        }
    }
    Ok(())
}

/// Checks one language's setting texts against the settings they translate, with the same
/// length limits.
pub fn validate_texts(settings: &[SettingSpec], texts: &BTreeMap<String, SettingTexts>) -> Result<(), String> {
    for (key, texts) in texts {
        let setting = settings.iter().find(|setting| setting.key() == key).ok_or_else(|| format!("there is no setting {key:?}"))?;
        let field = |name: &str| format!("setting {key:?}: {name}");
        let fits = (texts.unit.is_none() || matches!(setting, SettingSpec::Number { .. }))
            && (texts.placeholder.is_none() || setting.placeholder().is_some())
            && (texts.options.is_empty() || matches!(setting, SettingSpec::Select { .. }));
        if !fits {
            return Err(field("only number settings have a unit, text, url and secret settings a placeholder and select settings options"));
        }
        if let Some(label) = &texts.label {
            check_length(&field("label"), label, 1..=60)?;
        }
        if let Some(description) = &texts.description {
            check_length(&field("description"), description, 0..=200)?;
        }
        if let Some(placeholder) = &texts.placeholder {
            check_length(&field("placeholder"), placeholder, 0..=100)?;
        }
        if let Some(unit) = &texts.unit {
            check_length(&field("unit"), unit, 0..=10)?;
        }
        if let SettingSpec::Select { options, .. } = setting {
            for (value, label) in &texts.options {
                if !options.iter().any(|option| &option.value == value) {
                    return Err(field(&format!("there is no option {value:?}")));
                }
                check_length(&field("option label"), label, 1..=60)?;
            }
        }
    }
    Ok(())
}

/// Checks the length of a text shown in the settings window, and that it has no control
/// characters or characters that change the text direction, which could make it look like
/// something else, such as another widget's name followed by "(built in)".
pub(crate) fn check_length(what: &str, text: &str, allowed: RangeInclusive<usize>) -> Result<(), String> {
    let length = text.chars().count();
    if !allowed.contains(&length) {
        return Err(format!("{what} must be {} to {} characters long", allowed.start(), allowed.end()));
    }
    if let Some(bad) = text.chars().find(|&c| is_hidden_control(c)) {
        return Err(format!("{what} contains the control character U+{:04X}", u32::from(bad)));
    }
    Ok(())
}

/// Control characters, line and paragraph separators, and the bidirectional marks, embeddings,
/// overrides and isolates.
fn is_hidden_control(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{2028}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Every declared key with a valid value: the stored one, fixed up, or the default, with
/// secrets decrypted. Stored keys that are no longer declared are left out.
pub fn resolve(settings: &[SettingSpec], stored: Option<&Values>) -> Values {
    reveal(settings, &kept(settings, stored))
}

/// What is saved for these values: like `resolve`, but secrets stay encrypted, and a secret
/// stored in plain text (written into the file by hand) gets encrypted.
fn kept(settings: &[SettingSpec], stored: Option<&Values>) -> Values {
    settings
        .iter()
        .map(|setting| {
            let stored = stored.and_then(|values| values.get(setting.key()));
            let value = match setting {
                SettingSpec::Secret { .. } => match stored {
                    Some(value) if value.get(SECRET_FIELD).is_some() => Some(value.clone()),
                    Some(Value::String(text)) if !text.is_empty() => seal(text).ok(),
                    _ => None,
                },
                _ => stored.and_then(|value| setting.coerce(value)),
            };
            (setting.key().to_string(), value.unwrap_or_else(|| setting.default_value()))
        })
        .collect()
}

/// The values with their secrets decrypted.
fn reveal(settings: &[SettingSpec], kept: &Values) -> Values {
    let mut values = kept.clone();
    for setting in settings {
        if let SettingSpec::Secret { .. } = setting
            && let Some(value) = values.get_mut(setting.key())
            && value.get(SECRET_FIELD).is_some()
        {
            *value = Value::String(unseal(value).unwrap_or_default());
        }
    }
    values
}

/// Origins the user let a widget reach by filling in its `url` settings.
pub fn user_origins<R: tauri::Runtime>(app: &AppHandle<R>, manifest: &crate::widgets::WidgetManifest) -> Vec<String> {
    let Some(store) = app.try_state::<Store>() else { return Vec::new() };
    let stored = store.0.lock().unwrap();
    let stored = stored.get(&manifest.id);
    let mut origins: Vec<String> = manifest
        .settings
        .iter()
        .filter(|setting| matches!(setting, SettingSpec::Url { .. }))
        .filter_map(|setting| {
            let value = stored.and_then(|values| values.get(setting.key())).and_then(|value| setting.coerce(value));
            user_origin(value.unwrap_or_else(|| setting.default_value()).as_str()?)
        })
        .collect();
    origins.sort();
    origins.dedup();
    origins
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

/// Whether the user has changed any setting of this widget, installed or not.
pub fn has_values(app: &AppHandle, widget_id: &str) -> bool {
    app.state::<Store>().0.lock().unwrap().contains_key(widget_id)
}

/// Forgets a widget's values, if it has any, when a widget from another source takes its id;
/// `settings` are the new widget's, whose defaults are announced.
pub fn forget(app: &AppHandle, widget_id: &str, settings: &[SettingSpec]) -> Result<(), String> {
    if !has_values(app, widget_id) {
        return Ok(());
    }
    commit(app, &app.state::<Store>(), widget_id, None, &resolve(settings, None))
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
    let value = setting.coerce(&value).ok_or_else(|| match setting {
        SettingSpec::Url { .. } => {
            "use an https:// address; plain http:// works only on this computer and the local network".to_string()
        }
        _ => format!("{value} is not a valid value for {key:?}"),
    })?;
    let value = match (setting, value.as_str()) {
        (SettingSpec::Secret { .. }, Some(secret)) if !secret.is_empty() => seal(secret)?,
        _ => value,
    };
    let mut stored = kept(&settings, store.0.lock().unwrap().get(&widget_id));
    stored.insert(key, value);
    let values = reveal(&settings, &stored);
    commit(&app, &store, &widget_id, Some(stored), &values)?;
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
    fn reads_addresses_the_widget_may_reach() {
        let ok = |text: &str, expected: &str| assert_eq!(normalize_url(text).as_deref(), Some(expected), "{text}");
        ok("example.com", "https://example.com/");
        ok("https://example.com/feed.xml", "https://example.com/feed.xml");
        ok("wss://example.com:8443/ws", "wss://example.com:8443/ws");
        ok("http://192.168.1.5:8123", "http://192.168.1.5:8123/");
        ok("http://10.0.0.2", "http://10.0.0.2/");
        ok("http://100.101.1.2:80", "http://100.101.1.2/");
        ok("http://homeassistant.local:8123", "http://homeassistant.local:8123/");
        ok("http://nas:5000", "http://nas:5000/");
        ok("http://[fd00::1]:8123", "http://[fd00::1]:8123/");
        ok("ws://127.0.0.1:4455", "ws://127.0.0.1:4455/");
        for bad in [
            "http://example.com",
            "http://8.8.8.8",
            "http://172.32.0.1",
            "ftp://example.com",
            "file:///C:/x",
            "https://user:pass@example.com",
            "javascript:alert(1)",
            "https://",
        ] {
            assert_eq!(normalize_url(bad), None, "{bad} should be refused");
        }

        assert_eq!(user_origin("http://192.168.1.5:8123/api").as_deref(), Some("http://192.168.1.5:8123"));
        assert_eq!(user_origin("wss://example.com/socket").as_deref(), Some("https://example.com"));
        assert_eq!(user_origin("ws://localhost:4455").as_deref(), Some("http://localhost:4455"));
        assert_eq!(user_origin("http://127.0.0.1:9222"), None);
        assert_eq!(user_origin("http://localhost:6869"), None);
        assert_eq!(user_origin(""), None);
    }

    #[test]
    fn checks_address_and_secret_settings() {
        let good = parse(json!([
            { "key": "server", "type": "url", "label": "Server", "default": "", "placeholder": "http://homeassistant.local:8123" },
            { "key": "feed", "type": "url", "label": "Feed", "default": "https://example.com/feed.xml" },
            { "key": "token", "type": "secret", "label": "Token", "placeholder": "Paste a token" }
        ]))
        .unwrap();
        assert_eq!(Value::Object(resolve(&good, None)), json!({ "server": "", "feed": "https://example.com/feed.xml", "token": "" }));
        assert_eq!(good[0].coerce(&json!("http://192.168.0.2:8123")), Some(json!("http://192.168.0.2:8123/")));
        assert_eq!(good[0].coerce(&json!("http://example.com")), None);
        assert_eq!(good[0].coerce(&json!("  ")), Some(json!("")));
        for bad in [
            json!([{ "key": "u", "type": "url", "label": "U", "default": "http://example.com" }]),
            json!([{ "key": "u", "type": "url", "label": "U", "default": "example.com" }]),
            json!([{ "key": "s", "type": "secret", "label": "S", "default": "hunter2" }]),
        ] {
            assert!(parse(bad.clone()).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn keeps_secrets_encrypted() {
        let settings = parse(json!([{ "key": "token", "type": "secret", "label": "Token" }])).unwrap();
        let sealed = seal("ghp_secret").unwrap();
        let stored = json!({ "token": sealed.clone() });
        assert!(!sealed.to_string().contains("ghp_secret"));
        assert_eq!(Value::Object(resolve(&settings, stored.as_object())), json!({ "token": "ghp_secret" }));
        assert_eq!(kept(&settings, stored.as_object())["token"], sealed);
        // Written into the file by hand: read, and encrypted when saved.
        let plain = json!({ "token": "typed" });
        assert_eq!(resolve(&settings, plain.as_object())["token"], json!("typed"));
        assert!(kept(&settings, plain.as_object())["token"].get(SECRET_FIELD).is_some());
        // From another computer: cannot be decrypted, so it is empty.
        let foreign = json!({ "token": { SECRET_FIELD: "AAAA" } });
        assert_eq!(resolve(&settings, foreign.as_object())["token"], json!(""));
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
