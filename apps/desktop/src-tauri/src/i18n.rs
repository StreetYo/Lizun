//! Interface language (see ADR 0007). The texts live in `src/locales/<code>.json`, one file per
//! language, shared with the web UI. The core reads them for what it shows itself: the tray
//! menu, the settings window's title, the built-in widgets' settings and the errors that reach
//! the user. Texts a translation lacks come from English.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{LazyLock, RwLock};

use serde_json::Value;
use tauri::AppHandle;

/// Every translation: its code and its texts. English comes first and fills in for missing
/// texts. A new language needs its file in `src/locales` and a line here.
const SOURCES: &[(&str, &str)] = &[
    ("en", include_str!("../../src/locales/en.json")),
    ("ru", include_str!("../../src/locales/ru.json")),
];

/// The `language` preference that follows the Windows display language.
pub const SYSTEM: &str = "system";
const FALLBACK: &str = "en";

/// Texts of each language by dotted key, such as `tray.quit`.
static CATALOGS: LazyLock<Vec<HashMap<String, String>>> =
    LazyLock::new(|| SOURCES.iter().map(|(code, json)| flatten(code, json)).collect());

static CURRENT: RwLock<&'static str> = RwLock::new(FALLBACK);

fn flatten(code: &str, json: &str) -> HashMap<String, String> {
    fn collect(code: &str, value: &Value, key: String, texts: &mut HashMap<String, String>) {
        match value {
            Value::Object(entries) => {
                for (name, value) in entries {
                    let key = if key.is_empty() { name.clone() } else { format!("{key}.{name}") };
                    collect(code, value, key, texts);
                }
            }
            Value::String(text) => {
                texts.insert(key, text.clone());
            }
            _ => log::warn!("{code}.json: {key} is not a text"),
        }
    }
    let mut texts = HashMap::new();
    match serde_json::from_str(json) {
        Ok(value) => collect(code, &value, String::new(), &mut texts),
        Err(err) => log::error!("{code}.json is not valid JSON: {err}"),
    }
    texts
}

pub fn is_supported(code: &str) -> bool {
    SOURCES.iter().any(|(known, _)| *known == code)
}

/// The language a preference stands for: the code it names, or the system's language.
pub fn resolve(preference: &str) -> &'static str {
    SOURCES.iter().map(|(code, _)| *code).find(|code| *code == preference).unwrap_or_else(system_language)
}

/// The first of the user's display languages that Lizun is translated into, or English.
pub fn system_language() -> &'static str {
    crate::platform::preferred_languages().iter().find_map(|tag| matching(tag)).unwrap_or(FALLBACK)
}

/// The translation for a language tag such as `ru-RU`: the same tag, or else its language.
fn matching(tag: &str) -> Option<&'static str> {
    let primary = tag.split(['-', '_']).next().unwrap_or(tag);
    let codes = || SOURCES.iter().map(|(code, _)| *code);
    codes().find(|code| code.eq_ignore_ascii_case(tag)).or_else(|| codes().find(|code| code.eq_ignore_ascii_case(primary)))
}

/// Switches the core to the language the preference stands for and relabels what the core
/// shows itself.
pub fn apply(app: &AppHandle, preference: &str) {
    let language = resolve(preference);
    let previous = std::mem::replace(&mut *CURRENT.write().unwrap(), language);
    if previous != language {
        crate::system_tray::relabel(app);
        crate::settings::relabel(app);
    }
}

/// The text for `key` in the current language.
pub fn t(key: &str) -> String {
    text(*CURRENT.read().unwrap(), key)
}

/// The text for `key` with each `{name}` replaced by its value.
pub fn tf(key: &str, args: &[(&str, &dyn Display)]) -> String {
    fill(&t(key), args)
}

fn text(language: &str, key: &str) -> String {
    let index = SOURCES.iter().position(|(code, _)| *code == language).unwrap_or(0);
    CATALOGS[index].get(key).or_else(|| CATALOGS[0].get(key)).cloned().unwrap_or_else(|| {
        log::warn!("no text for {key}");
        key.to_string()
    })
}

/// Replaces `{name}` placeholders in one pass, so a value that contains braces stays as it is.
fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut filled = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        filled.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let value = after
            .find('}')
            .and_then(|end| args.iter().find(|(name, _)| *name == &after[..end]).map(|(_, value)| (value, end)));
        match value {
            Some((value, end)) => {
                filled.push_str(&value.to_string());
                rest = &after[end + 1..];
            }
            None => {
                filled.push('{');
                rest = after;
            }
        }
    }
    filled.push_str(rest);
    filled
}

/// The language the system would pick; the web UI resolves the `system` preference with it.
#[tauri::command]
pub fn i18n_system_language() -> &'static str {
    system_language()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const PLURAL_FORMS: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

    /// `inDays_few` and `inDays_other` are forms of `inDays`.
    fn base_key(key: &str) -> &str {
        key.rsplit_once('_').filter(|(_, form)| PLURAL_FORMS.contains(form)).map_or(key, |(base, _)| base)
    }

    fn placeholders(text: &str) -> BTreeSet<String> {
        text.split('{').skip(1).filter_map(|part| part.split_once('}')).map(|(name, _)| name.to_string()).collect()
    }

    #[test]
    fn every_locale_file_is_listed() {
        let dir = crate::widgets::repo_path("apps/desktop/src/locales");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
            .collect();
        files.sort();
        let mut listed: Vec<String> = SOURCES.iter().map(|(code, _)| code.to_string()).collect();
        listed.sort();
        assert_eq!(files, listed, "add the new language to SOURCES in i18n.rs");
    }

    #[test]
    fn translations_have_every_text_with_the_same_placeholders() {
        let english = &CATALOGS[0];
        for ((code, json), texts) in SOURCES.iter().zip(CATALOGS.iter()) {
            assert!(serde_json::from_str::<Value>(json).is_ok(), "{code}.json is not valid JSON");
            for key in english.keys() {
                let base = base_key(key);
                let has = texts.contains_key(key) || (base != key && texts.contains_key(&format!("{base}_other")));
                assert!(has, "{code}.json has no {key}");
            }
            for (key, text) in texts {
                let base = base_key(key);
                let source = english.get(key).or_else(|| english.get(&format!("{base}_other")));
                let source = source.unwrap_or_else(|| panic!("{code}.json has {key}, which English does not have"));
                let extra: Vec<_> = placeholders(text).difference(&placeholders(source)).cloned().collect();
                assert!(extra.is_empty(), "{code}.json: {key} uses {extra:?}, which the English text does not have");
            }
        }
    }

    #[test]
    fn falls_back_to_english_and_fills_in_values() {
        assert_eq!(text("ru", "tray.quit"), "Выйти из Lizun");
        assert_eq!(text("xx", "tray.quit"), "Quit Lizun");
        assert_eq!(text("en", "no.such.text"), "no.such.text");
        let port: &dyn Display = &6869;
        let error: &dyn Display = &"{port} in use";
        assert_eq!(fill("Port {port}: {error} {unknown}", &[("port", port), ("error", error)]), "Port 6869: {port} in use {unknown}");
    }

    #[test]
    fn matches_system_language_tags() {
        assert_eq!(matching("ru-RU"), Some("ru"));
        assert_eq!(matching("en-GB"), Some("en"));
        assert_eq!(matching("EN"), Some("en"));
        assert_eq!(matching("de-DE"), None);
        assert_eq!(resolve("ru"), "ru");
    }
}
