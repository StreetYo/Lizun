//! Data widgets keep with `storage` in the SDK: one file per key in
//! `%APPDATA%\app.lizun.desktop\widget-data\<widget id>\`.
//!
//! The island's `localStorage` held this data before; it allows a few megabytes for all
//! widgets together, too little for a widget with real data such as flash card decks. The
//! island moves a key from there the first time it is read (see `lib/widgets/host.ts`).
//! Removing a widget keeps its data, like its settings, for a later reinstall.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};

const DIR_NAME: &str = "widget-data";
/// Larger values are refused.
pub const MAX_VALUE_BYTES: usize = 1024 * 1024;
/// A widget cannot keep more than this in total.
pub const MAX_WIDGET_BYTES: u64 = 50 * 1024 * 1024;
/// Keys are at most this long, in characters.
const MAX_KEY_CHARS: usize = 128;

/// Writes one at a time, so the size check and the write cannot interleave.
#[derive(Default)]
pub struct Writes(Mutex<()>);

/// Returns the stored JSON text, or `None` when the key was never set.
#[tauri::command]
pub fn widget_storage_get(app: AppHandle, widget_id: String, key: String) -> Result<Option<String>, String> {
    let dir = widget_dir(&app, &widget_id)?;
    read(&dir, &key)
}

/// Stores `value`, a JSON text.
#[tauri::command]
pub fn widget_storage_set(app: AppHandle, widget_id: String, key: String, value: String) -> Result<(), String> {
    let dir = widget_dir(&app, &widget_id)?;
    let writes = app.state::<Writes>();
    let _write = writes.0.lock().unwrap_or_else(|e| e.into_inner());
    write(&dir, &key, &value)
}

#[tauri::command]
pub fn widget_storage_remove(app: AppHandle, widget_id: String, key: String) -> Result<(), String> {
    let dir = widget_dir(&app, &widget_id)?;
    let writes = app.state::<Writes>();
    let _write = writes.0.lock().unwrap_or_else(|e| e.into_inner());
    remove(&dir, &key)
}

/// Deletes everything a widget kept, when a widget from another source takes its id (see
/// `widget_install.rs`), so the newcomer cannot read it. The id comes from a validated
/// manifest, so it is a plain folder name.
pub fn forget<R: Runtime>(app: &AppHandle<R>, widget_id: &str) -> Result<(), String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    match fs::remove_dir_all(data.join(DIR_NAME).join(widget_id)) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err.to_string()),
        _ => Ok(()),
    }
}

fn widget_dir<R: Runtime>(app: &AppHandle<R>, widget_id: &str) -> Result<PathBuf, String> {
    // Only ids of loaded widgets, which are checked to be safe folder names.
    crate::widgets::find(app, widget_id).ok_or("unknown widget")?;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(data.join(DIR_NAME).join(widget_id))
}

/// Keys up to this many UTF-8 bytes are named by their hex; the name stays under the 255
/// characters a file name may have.
const MAX_HEX_KEY_BYTES: usize = 100;

/// The key's file: its UTF-8 bytes in hex, so any key is a valid file name on every system.
/// Longer keys (128 Cyrillic letters are 256 bytes) are named by their SHA-256 instead, after a
/// `~` that no hex name has.
fn key_path(dir: &Path, key: &str) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    if key.is_empty() || key.chars().count() > MAX_KEY_CHARS {
        return Err("invalid storage key".into());
    }
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let name = if key.len() <= MAX_HEX_KEY_BYTES {
        hex(key.as_bytes())
    } else {
        format!("~{}", hex(&Sha256::digest(key.as_bytes())))
    };
    Ok(dir.join(format!("{name}.json")))
}

fn read(dir: &Path, key: &str) -> Result<Option<String>, String> {
    match fs::read_to_string(key_path(dir, key)?) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

fn write(dir: &Path, key: &str, value: &str) -> Result<(), String> {
    let path = key_path(dir, key)?;
    if value.len() > MAX_VALUE_BYTES {
        return Err(format!("value exceeds {} KB", MAX_VALUE_BYTES / 1024));
    }
    serde_json::from_str::<serde_json::Value>(value).map_err(|_| "value is not JSON")?;
    let replaced = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if used(dir) - replaced + value.len() as u64 > MAX_WIDGET_BYTES {
        return Err(format!("the widget's data would exceed {} MB", MAX_WIDGET_BYTES / 1024 / 1024));
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Write a copy and swap it in, so a crash cannot leave half a value.
    let partial = path.with_extension("json.partial");
    fs::write(&partial, value).map_err(|e| e.to_string())?;
    fs::rename(&partial, &path).map_err(|e| e.to_string())
}

fn remove(dir: &Path, key: &str) -> Result<(), String> {
    match fs::remove_file(key_path(dir, key)?) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err.to_string()),
        _ => Ok(()),
    }
}

/// Bytes the widget keeps now.
fn used(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries.flatten().filter_map(|entry| entry.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lizun-storage-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn keeps_values_by_key() {
        let dir = temp_dir("values");
        assert_eq!(read(&dir, "deck").unwrap(), None);
        write(&dir, "deck", r#"{"cards":[1,2]}"#).unwrap();
        write(&dir, "Deck", "3").unwrap();
        write(&dir, "колода/1 ../x", "\"ok\"").unwrap();
        assert_eq!(read(&dir, "deck").unwrap().as_deref(), Some(r#"{"cards":[1,2]}"#));
        assert_eq!(read(&dir, "Deck").unwrap().as_deref(), Some("3"));
        assert_eq!(read(&dir, "колода/1 ../x").unwrap().as_deref(), Some("\"ok\""));
        remove(&dir, "deck").unwrap();
        remove(&dir, "deck").unwrap();
        assert_eq!(read(&dir, "deck").unwrap(), None);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeps_the_longest_keys() {
        let dir = temp_dir("long keys");
        let cyrillic = "я".repeat(MAX_KEY_CHARS);
        let ascii = "k".repeat(MAX_KEY_CHARS);
        write(&dir, &cyrillic, "1").unwrap();
        write(&dir, &ascii, "2").unwrap();
        write(&dir, &"k".repeat(MAX_HEX_KEY_BYTES), "3").unwrap();
        assert_eq!(read(&dir, &cyrillic).unwrap().as_deref(), Some("1"));
        assert_eq!(read(&dir, &ascii).unwrap().as_deref(), Some("2"));
        assert_eq!(read(&dir, &"k".repeat(MAX_HEX_KEY_BYTES)).unwrap().as_deref(), Some("3"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refuses_bad_keys_and_values() {
        let dir = temp_dir("limits");
        assert!(write(&dir, "", "1").is_err());
        assert!(write(&dir, &"k".repeat(MAX_KEY_CHARS + 1), "1").is_err());
        assert!(write(&dir, "k", "not json").is_err());
        let big = format!("\"{}\"", "x".repeat(MAX_VALUE_BYTES));
        assert!(write(&dir, "k", &big).unwrap_err().contains("exceeds"));
        assert!(!dir.exists());
    }

    #[test]
    fn limits_the_total_per_widget() {
        let dir = temp_dir("total");
        let value = format!("\"{}\"", "x".repeat(MAX_VALUE_BYTES - 2));
        let fits = (MAX_WIDGET_BYTES / MAX_VALUE_BYTES as u64) as usize;
        for i in 0..fits {
            write(&dir, &format!("k{i}"), &value).unwrap();
        }
        assert!(write(&dir, "one more", &value).unwrap_err().contains("would exceed"));
        // Replacing a value counts only the difference.
        write(&dir, "k0", &value).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }
}
