//! Global shortcuts: one opens and closes the island, three control the music. They work in any
//! app, so a shortcut must not take a key or combination that Windows or apps need; `check`
//! refuses those. The platform backend registers keyboard shortcuts with the system and watches
//! the side mouse buttons.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::media::MediaCommand;

/// Shortcuts that could not be registered changed.
pub const EVENT_STATUS: &str = "hotkeys://status";
/// Sent to the island when its shortcut is pressed.
pub const EVENT_TOGGLE_ISLAND: &str = "hotkeys://toggle-island";

/// What a shortcut does, in the order of the fields of `Hotkeys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    ToggleIsland,
    PlayPause,
    NextTrack,
    PreviousTrack,
}

const ACTIONS: [Action; 4] = [Action::ToggleIsland, Action::PlayPause, Action::NextTrack, Action::PreviousTrack];

/// The shortcut of each action, written like `Ctrl+Alt+Space` (see `Shortcut`); `None` turns
/// it off. Missing fields take their defaults; a shortcut `check` refuses is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkeys {
    pub toggle_island: Option<String>,
    pub play_pause: Option<String>,
    pub next_track: Option<String>,
    pub previous_track: Option<String>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self { toggle_island: Some("Alt+Shift+Space".into()), play_pause: None, next_track: None, previous_track: None }
    }
}

impl Hotkeys {
    fn slots(&self) -> [&Option<String>; 4] {
        [&self.toggle_island, &self.play_pause, &self.next_track, &self.previous_track]
    }

    fn slots_mut(&mut self) -> [&mut Option<String>; 4] {
        [&mut self.toggle_island, &mut self.play_pause, &mut self.next_track, &mut self.previous_track]
    }

    /// Writes every shortcut the same way and drops the ones `check` refuses, and a shortcut
    /// that an earlier action already has.
    pub fn cleaned(mut self) -> Self {
        let mut taken = Vec::new();
        for slot in self.slots_mut() {
            let shortcut = slot.as_deref().and_then(|text| Shortcut::parse(text).ok()).filter(|s| check(s).is_ok());
            *slot = match shortcut {
                Some(shortcut) if !taken.contains(&shortcut) => {
                    taken.push(shortcut);
                    Some(shortcut.to_string())
                }
                _ => None,
            };
        }
        self
    }

    fn shortcuts(&self) -> Vec<Option<Shortcut>> {
        self.slots().iter().map(|slot| slot.as_deref().and_then(|text| Shortcut::parse(text).ok())).collect()
    }
}

/// The key of a shortcut: a keyboard key by its `KeyboardEvent.code` name, or a side mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Keyboard(&'static str),
    MouseBack,
    MouseForward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: Key,
}

/// Keyboard keys a shortcut can use, by their `KeyboardEvent.code` names. Escape, Tab, Enter,
/// Backspace, Delete, the lock keys, Print Screen, Pause, the menu key and the media keys are
/// missing on purpose: Windows and apps need them with any modifier.
const KEYS: &[&str] = &[
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM",
    "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ",
    "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9",
    "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote", "Backquote", "Comma",
    "Period", "Slash", "IntlBackslash",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
    "Space", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown", "Insert",
    "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7", "Numpad8",
    "Numpad9", "NumpadAdd", "NumpadSubtract", "NumpadMultiply", "NumpadDivide", "NumpadDecimal",
];

const MOUSE_BACK: &str = "MouseBack";
const MOUSE_FORWARD: &str = "MouseForward";

impl Shortcut {
    /// Reads `Ctrl+Alt+Space`: modifiers in any order, then the key. Fails with `Refusal::Key`.
    pub fn parse(text: &str) -> Result<Self, Refusal> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = parts.pop().ok_or(Refusal::Key)?;
        let key = match key {
            MOUSE_BACK => Key::MouseBack,
            MOUSE_FORWARD => Key::MouseForward,
            _ => Key::Keyboard(KEYS.iter().find(|&&name| name == key).ok_or(Refusal::Key)?),
        };
        let mut shortcut = Shortcut { ctrl: false, alt: false, shift: false, win: false, key };
        for part in parts {
            let flag = match part {
                "Ctrl" => &mut shortcut.ctrl,
                "Alt" => &mut shortcut.alt,
                "Shift" => &mut shortcut.shift,
                "Win" => &mut shortcut.win,
                _ => return Err(Refusal::Key),
            };
            if *flag {
                return Err(Refusal::Key);
            }
            *flag = true;
        }
        Ok(shortcut)
    }

    fn modifiers(&self) -> usize {
        [self.ctrl, self.alt, self.shift, self.win].into_iter().filter(|&on| on).count()
    }

    /// Whether the modifiers are exactly these.
    fn held(&self, ctrl: bool, alt: bool, shift: bool, win: bool) -> bool {
        (self.ctrl, self.alt, self.shift, self.win) == (ctrl, alt, shift, win)
    }
}

impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(match self.key {
            Key::Keyboard(code) => code,
            Key::MouseBack => MOUSE_BACK,
            Key::MouseForward => MOUSE_FORWARD,
        })
    }
}

/// Why a shortcut cannot be used; the settings explain each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Refusal {
    /// Not a key a shortcut can use, such as Escape, Tab or a media key.
    Key,
    /// A key with no modifier or only Shift, which types or does something in apps.
    Modifier,
    /// A key with one modifier, such as Ctrl+C or Win+D, which Windows and apps take.
    OneModifier,
    /// Ctrl+Alt with a letter, digit or sign: on many layouts that is AltGr and types a character.
    AltGr,
    /// A combination Windows keeps for itself.
    System,
    /// A combination nearly every app uses, such as Ctrl+Shift+Z.
    Apps,
}

/// The keys that type a character, which AltGr (Ctrl+Alt) changes on many layouts.
fn types_character(code: &str) -> bool {
    code.starts_with("Key")
        || code.starts_with("Digit")
        || matches!(
            code,
            "Minus" | "Equal" | "BracketLeft" | "BracketRight" | "Backslash" | "Semicolon" | "Quote" | "Backquote"
                | "Comma" | "Period" | "Slash" | "IntlBackslash"
        )
}

/// F13–F24, which keyboards do not have but macro keypads and mouse software send.
fn extra_function_key(code: &str) -> bool {
    code.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (13..=24).contains(&n))
}

/// Whether a shortcut can be used. The side mouse buttons and F13–F24 work alone; any other key
/// needs at least two modifiers, one of them Ctrl, Alt or Win.
pub fn check(shortcut: &Shortcut) -> Result<(), Refusal> {
    let code = match shortcut.key {
        Key::MouseBack | Key::MouseForward => return Ok(()),
        Key::Keyboard(code) => code,
    };
    let s = shortcut;
    if extra_function_key(code) {
        // The Copilot key sends Shift+Win+F23.
        return if code == "F23" && s.held(false, false, true, true) { Err(Refusal::System) } else { Ok(()) };
    }
    if !(s.ctrl || s.alt || s.win) {
        return Err(Refusal::Modifier);
    }
    if s.modifiers() == 1 {
        return Err(Refusal::OneModifier);
    }
    if s.ctrl && s.alt && !s.win && types_character(code) {
        return Err(Refusal::AltGr);
    }
    if reserved_by_windows(s, code) {
        return Err(Refusal::System);
    }
    // Reopen a closed tab, a private window, redo, paste as plain text.
    if s.held(true, false, true, false) && matches!(code, "KeyT" | "KeyN" | "KeyZ" | "KeyV") {
        return Err(Refusal::Apps);
    }
    Ok(())
}

/// Combinations with two or more modifiers that Windows, its graphics drivers or Remote Desktop
/// keep for themselves. Windows also refuses to register most of its own shortcuts, which the
/// settings then show as taken; these are the ones it does not refuse, or that do harm.
fn reserved_by_windows(s: &Shortcut, code: &str) -> bool {
    let digit = code.starts_with("Digit");
    let arrow = code.starts_with("Arrow");
    let letter = code.starts_with("Key");
    // Ctrl+Alt+arrows turn the screen with some graphics drivers; Ctrl+Alt+End is
    // Ctrl+Alt+Delete in Remote Desktop, which also takes Home and the numpad signs.
    (s.held(true, true, false, false)
        && (arrow || matches!(code, "End" | "Home" | "NumpadAdd" | "NumpadSubtract")))
        // Win+Shift: screenshot, screen recording, windows between monitors, minimized windows
        // back, input language, a new window of a taskbar app.
        || (s.held(false, false, true, true)
            && (digit || arrow || matches!(code, "KeyS" | "KeyR" | "KeyM" | "KeyC" | "KeyV" | "Space")))
        // Win+Ctrl: virtual desktops, color filters, Narrator, on-screen keyboard, Quick Assist,
        // input language, sound output, taskbar apps.
        || (s.held(true, false, false, true)
            && (digit
                || matches!(
                    code,
                    "KeyD" | "F4" | "ArrowLeft" | "ArrowRight" | "KeyC" | "KeyN" | "KeyO" | "KeyQ" | "KeyS" | "KeyF"
                        | "KeyV" | "Space"
                )))
        // Win+Ctrl+Shift+B restarts the graphics driver.
        || (s.held(true, false, true, true) && (digit || code == "KeyB"))
        // Win+Alt: Game Bar, HDR, microphone, date and time, voice typing, jump lists.
        || (s.held(false, true, false, true)
            && (digit || arrow || matches!(code, "KeyR" | "KeyG" | "KeyB" | "KeyK" | "KeyD" | "KeyH")))
        // Ctrl+Alt+Shift+Win with a letter is the Office key.
        || (s.held(true, true, true, true) && letter)
}

pub struct HotkeyState {
    hotkeys: Hotkeys,
    /// True while the settings record a new shortcut, so pressing a set one does not fire it.
    paused: bool,
    /// Actions whose shortcut another app or Windows already has.
    failed: Vec<Action>,
    listener: Option<crate::platform::HotkeyListener>,
}

pub fn start(app: &AppHandle, hotkeys: &Hotkeys) {
    let handle = app.clone();
    let listener = crate::platform::spawn_hotkey_listener(move |index| {
        let Some(&action) = ACTIONS.get(index) else { return };
        let app = handle.clone();
        // Media control can wait on the player; the listener thread must stay free.
        tauri::async_runtime::spawn_blocking(move || run(&app, action));
    });
    app.manage(Mutex::new(HotkeyState { hotkeys: hotkeys.clone(), paused: false, failed: Vec::new(), listener }));
    apply(app);
}

/// Takes new shortcuts from the preferences.
pub fn configure(app: &AppHandle, hotkeys: &Hotkeys) {
    let state = app.state::<Mutex<HotkeyState>>();
    {
        let mut state = state.lock().unwrap();
        if state.hotkeys == *hotkeys {
            return;
        }
        state.hotkeys = hotkeys.clone();
    }
    apply(app);
}

/// Registers the shortcuts again and tells the settings which of them are taken.
fn apply(app: &AppHandle) {
    let state = app.state::<Mutex<HotkeyState>>();
    let mut state = state.lock().unwrap();
    let Some(listener) = &state.listener else { return };
    let shortcuts = if state.paused { vec![None; ACTIONS.len()] } else { state.hotkeys.shortcuts() };
    let registered = listener.set(shortcuts.clone());
    let failed: Vec<Action> = ACTIONS
        .iter()
        .zip(shortcuts.iter().zip(registered))
        .filter(|(_, (shortcut, registered))| shortcut.is_some() && !registered)
        .map(|(&action, _)| action)
        .collect();
    // While paused nothing is registered, so the last known state stays.
    if !state.paused && failed != state.failed {
        for action in &failed {
            log::warn!("the shortcut for {action:?} is taken by another app");
        }
        state.failed = failed.clone();
        if let Err(err) = app.emit(EVENT_STATUS, &failed) {
            log::warn!("failed to announce the shortcut status: {err}");
        }
    }
}

fn run(app: &AppHandle, action: Action) {
    let command = match action {
        Action::ToggleIsland => {
            if let Err(err) = app.emit_to(crate::island::ISLAND_LABEL, EVENT_TOGGLE_ISLAND, ()) {
                log::warn!("failed to toggle the island: {err}");
            }
            return;
        }
        Action::PlayPause => MediaCommand::TogglePlayPause,
        Action::NextTrack => MediaCommand::Next,
        Action::PreviousTrack => MediaCommand::Previous,
    };
    if let Err(err) = crate::media::control(app, command) {
        log::debug!("media shortcut did nothing: {err}");
    }
}

/// Whether a shortcut can be used, written the way the preferences keep it.
#[tauri::command]
pub fn hotkeys_check(shortcut: String) -> Result<String, Refusal> {
    let shortcut = Shortcut::parse(&shortcut)?;
    check(&shortcut)?;
    Ok(shortcut.to_string())
}

/// Turns the shortcuts off while the settings record a new one, and back on afterwards.
#[tauri::command]
pub fn hotkeys_pause(app: AppHandle, paused: bool) {
    {
        let state = app.state::<Mutex<HotkeyState>>();
        let mut state = state.lock().unwrap();
        if state.paused == paused {
            return;
        }
        state.paused = paused;
    }
    apply(&app);
}

/// Actions whose shortcut another app or Windows already has.
#[tauri::command]
pub fn hotkeys_status(state: State<'_, Mutex<HotkeyState>>) -> Vec<Action> {
    state.lock().unwrap().failed.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(text: &str) -> Result<String, Refusal> {
        hotkeys_check(text.into())
    }

    #[test]
    fn writes_shortcuts_one_way() {
        assert_eq!(checked("Shift+Alt+Ctrl+Space"), Ok("Ctrl+Alt+Shift+Space".into()));
        assert_eq!(checked("Win+Ctrl+KeyK"), Ok("Ctrl+Win+KeyK".into()));
        assert_eq!(checked("MouseBack"), Ok("MouseBack".into()));
        assert_eq!(checked("Ctrl+MouseForward"), Ok("Ctrl+MouseForward".into()));
        assert_eq!(checked("Ctrl+Ctrl+Alt+Space"), Err(Refusal::Key));
        assert_eq!(checked("Hyper+Alt+Space"), Err(Refusal::Key));
        assert_eq!(checked(""), Err(Refusal::Key));
    }

    #[test]
    fn refuses_keys_windows_and_apps_need() {
        for key in ["Escape", "Tab", "Enter", "Backspace", "Delete", "CapsLock", "PrintScreen", "MediaPlayPause", "MouseLeft", "WheelUp"] {
            assert_eq!(checked(&format!("Ctrl+Alt+Shift+{key}")), Err(Refusal::Key), "{key}");
        }
    }

    #[test]
    fn needs_modifiers_for_ordinary_keys() {
        assert_eq!(checked("KeyA"), Err(Refusal::Modifier));
        assert_eq!(checked("F5"), Err(Refusal::Modifier));
        assert_eq!(checked("Shift+KeyA"), Err(Refusal::Modifier));
        assert_eq!(checked("Ctrl+KeyC"), Err(Refusal::OneModifier));
        assert_eq!(checked("Alt+F4"), Err(Refusal::OneModifier));
        assert_eq!(checked("Win+KeyD"), Err(Refusal::OneModifier));
        assert_eq!(checked("F13"), Ok("F13".into()));
        assert_eq!(checked("Alt+F24"), Ok("Alt+F24".into()));
        assert_eq!(checked("Ctrl+Shift+KeyK"), Ok("Ctrl+Shift+KeyK".into()));
        assert_eq!(checked("Alt+Shift+ArrowRight"), Ok("Alt+Shift+ArrowRight".into()));
    }

    #[test]
    fn keeps_altgr_typing() {
        assert_eq!(checked("Ctrl+Alt+KeyE"), Err(Refusal::AltGr));
        assert_eq!(checked("Ctrl+Alt+Shift+Digit2"), Err(Refusal::AltGr));
        assert_eq!(checked("Ctrl+Alt+Space"), Ok("Ctrl+Alt+Space".into()));
        assert_eq!(checked("Ctrl+Alt+Win+KeyE"), Ok("Ctrl+Alt+Win+KeyE".into()));
    }

    #[test]
    fn refuses_combinations_windows_keeps() {
        for text in [
            "Ctrl+Alt+ArrowUp",
            "Ctrl+Alt+End",
            "Shift+Win+KeyS",
            "Shift+Win+ArrowLeft",
            "Ctrl+Win+ArrowRight",
            "Ctrl+Win+KeyD",
            "Ctrl+Shift+Win+KeyB",
            "Alt+Win+KeyR",
            "Ctrl+Alt+Shift+Win+KeyW",
            "Shift+Win+F23",
        ] {
            assert_eq!(checked(text), Err(Refusal::System), "{text}");
        }
        assert_eq!(checked("Ctrl+Shift+KeyZ"), Err(Refusal::Apps));
        assert_eq!(checked("Ctrl+Alt+Shift+ArrowUp"), Ok("Ctrl+Alt+Shift+ArrowUp".into()));
    }

    #[test]
    fn cleans_up_the_preferences() {
        let messy = Hotkeys {
            toggle_island: Some("Alt+Ctrl+Space".into()),
            play_pause: Some("Ctrl+KeyC".into()),
            next_track: Some("Ctrl+Alt+Space".into()),
            previous_track: Some("MouseBack".into()),
        };
        assert_eq!(
            messy.cleaned(),
            Hotkeys {
                toggle_island: Some("Ctrl+Alt+Space".into()),
                play_pause: None,
                next_track: None,
                previous_track: Some("MouseBack".into()),
            }
        );
        let empty: Hotkeys = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Hotkeys::default());
        let off: Hotkeys = serde_json::from_str(r#"{"toggleIsland":null}"#).unwrap();
        assert_eq!(off.toggle_island, None);
    }
}
