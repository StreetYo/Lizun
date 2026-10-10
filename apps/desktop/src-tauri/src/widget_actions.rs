//! Things a widget asks Lizun to do outside its sandbox when the user clicks something in it:
//! open a link in the browser (`links`) and put text on the clipboard (`clipboard:write`).
//!
//! The island hands these on only while the user points at the widget in the open island (see
//! `mayAct` in src/lib/widgets/host.ts); this side checks the permission again, what is passed,
//! and how often, so a widget cannot open tabs or overwrite the clipboard in a loop.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Url, WebviewWindow};

pub const LINKS: &str = "links";
pub const CLIPBOARD_WRITE: &str = "clipboard:write";
/// Longest link a widget may open.
const MAX_URL: usize = 4096;
/// Longest text a widget may copy, in bytes.
const MAX_TEXT: usize = 1024 * 1024;
/// A widget may do each action once in this time.
const MIN_INTERVAL: Duration = Duration::from_millis(500);

/// When each widget last did each action.
static LAST: Mutex<Option<HashMap<(String, &'static str), Instant>>> = Mutex::new(None);

fn check(app: &AppHandle, widget_id: &str, permission: &'static str) -> Result<(), String> {
    let manifest = crate::widgets::find(app, widget_id).ok_or("unknown widget")?;
    if !manifest.permissions.iter().any(|p| p == permission) {
        return Err(format!("this needs the \"{permission}\" permission in manifest.json"));
    }
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let last = last.get_or_insert_with(HashMap::new);
    let key = (widget_id.to_string(), permission);
    if last.get(&key).is_some_and(|at| at.elapsed() < MIN_INTERVAL) {
        return Err("too many requests at once".into());
    }
    last.insert(key, Instant::now());
    Ok(())
}

/// Opens an `http` or `https` link in the default browser.
#[tauri::command]
pub fn widget_open_link(app: AppHandle, widget_id: String, url: String) -> Result<(), String> {
    let url = web_link(&url)?;
    check(&app, &widget_id, LINKS)?;
    crate::platform::open_url(url.as_str())
}

#[tauri::command]
pub fn widget_clipboard_write(app: AppHandle, window: WebviewWindow, widget_id: String, text: String) -> Result<(), String> {
    if text.len() > MAX_TEXT {
        return Err(format!("the text exceeds {} MB", MAX_TEXT / 1024 / 1024));
    }
    check(&app, &widget_id, CLIPBOARD_WRITE)?;
    crate::platform::write_clipboard_text(&window, &text)
}

/// The link as the browser would get it, or why it is refused: only web pages, so a widget
/// cannot start programs or open files through the shell.
fn web_link(url: &str) -> Result<Url, String> {
    if url.len() > MAX_URL {
        return Err(format!("the link is longer than {MAX_URL} characters"));
    }
    let parsed = Url::parse(url).map_err(|err| format!("invalid link: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("only http and https links can be opened".into());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_web_links() {
        assert!(web_link("https://example.com/a?b=c").is_ok());
        assert_eq!(web_link("http://example.com/a b").unwrap().as_str(), "http://example.com/a%20b");
        for bad in ["file:///C:/Windows/notepad.exe", "ms-settings:", "javascript:alert(1)", "C:\\x.exe", "https://", "x"] {
            assert!(web_link(bad).is_err(), "{bad} should be refused");
        }
        assert!(web_link(&format!("https://example.com/{}", "a".repeat(MAX_URL))).is_err());
    }
}
