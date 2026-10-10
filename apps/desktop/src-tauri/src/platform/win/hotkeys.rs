//! Global shortcuts on Windows. A thread of its own registers keyboard shortcuts with
//! `RegisterHotKey`, which leaves every other keystroke alone and fails when another app has the
//! combination. The side mouse buttons have no such API, so while a shortcut uses one, a
//! low-level mouse hook watches for them; it lets every other mouse event through untouched.

use std::cell::RefCell;
use std::sync::mpsc;
use std::time::Duration;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MAPVK_VSC_TO_VK, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    MapVirtualKeyW, RegisterHotKey, UnregisterHotKey, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP, WM_HOTKEY, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1,
    XBUTTON2,
};

use crate::hotkeys::{Key, Shortcut};

/// Tells the listener thread that new shortcuts are waiting in its channel.
const WM_RECONFIGURE: u32 = WM_APP + 1;

type Request = (Vec<Option<Shortcut>>, mpsc::Sender<Vec<bool>>);

pub struct HotkeyListener {
    thread: u32,
    requests: mpsc::Sender<Request>,
}

impl HotkeyListener {
    /// Replaces the shortcuts; shortcut `i` calls `on_press(i)`. Returns for each whether it
    /// could be registered.
    pub fn set(&self, shortcuts: Vec<Option<Shortcut>>) -> Vec<bool> {
        let count = shortcuts.len();
        let (reply, replied) = mpsc::channel();
        if self.requests.send((shortcuts, reply)).is_err()
            // SAFETY: posting a message without pointers to a thread of this process.
            || unsafe { PostThreadMessageW(self.thread, WM_RECONFIGURE, WPARAM(0), LPARAM(0)) }.is_err()
        {
            return vec![false; count];
        }
        replied.recv_timeout(Duration::from_secs(2)).unwrap_or_else(|_| vec![false; count])
    }
}

/// Starts the listener thread with no shortcuts.
pub fn spawn_hotkey_listener(on_press: impl Fn(usize) + Send + 'static) -> Option<HotkeyListener> {
    let (requests, incoming) = mpsc::channel::<Request>();
    let (ready, started) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("hotkeys".into()).spawn(move || {
        let mut msg = MSG::default();
        // SAFETY: `msg` is a valid MSG. Peeking creates the thread's message queue, so messages
        // posted from now on arrive.
        unsafe {
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let _ = ready.send(GetCurrentThreadId());
        }
        LISTENER.with_borrow_mut(|listener| {
            *listener = Some(Listener { on_press: Some(Box::new(on_press)), ..Listener::default() });
        });
        // SAFETY: `msg` is a valid MSG; the loop ends when the queue is closed or fails.
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
            match msg.message {
                WM_HOTKEY => press(msg.wParam.0.wrapping_sub(1)),
                WM_RECONFIGURE => {
                    while let Ok((shortcuts, reply)) = incoming.try_recv() {
                        let _ = reply.send(LISTENER.with_borrow_mut(|l| l.as_mut().map(|l| l.set(&shortcuts))).unwrap_or_default());
                    }
                }
                _ => {}
            }
        }
    });
    if let Err(err) = spawned {
        log::warn!("could not start the shortcut listener: {err}");
        return None;
    }
    Some(HotkeyListener { thread: started.recv().ok()?, requests })
}

#[derive(Default)]
struct Listener {
    on_press: Option<Box<dyn Fn(usize)>>,
    /// Ids passed to `RegisterHotKey`: the index of the shortcut plus one.
    registered: Vec<i32>,
    /// Shortcuts on a side mouse button, with their index.
    mouse: Vec<(usize, Shortcut)>,
    hook: Option<HHOOK>,
    /// For each side button, whether its press was a shortcut, so its release is held back too.
    held_back: [bool; 2],
}

thread_local! {
    /// The listener of the hotkey thread, which also runs the mouse hook.
    static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
}

fn press(index: usize) {
    LISTENER.with_borrow(|listener| {
        if let Some(on_press) = listener.as_ref().and_then(|l| l.on_press.as_ref()) {
            on_press(index);
        }
    });
}

impl Listener {
    fn set(&mut self, shortcuts: &[Option<Shortcut>]) -> Vec<bool> {
        for id in self.registered.drain(..) {
            // SAFETY: unregisters a thread hotkey this thread registered.
            let _ = unsafe { UnregisterHotKey(None, id) };
        }
        self.mouse.clear();
        let mut registered = vec![false; shortcuts.len()];
        for (index, shortcut) in shortcuts.iter().enumerate() {
            let Some(shortcut) = shortcut else { continue };
            match shortcut.key {
                Key::MouseBack | Key::MouseForward => self.mouse.push((index, *shortcut)),
                Key::Keyboard(code) => {
                    let Some(vk) = virtual_key(code) else { continue };
                    let id = index as i32 + 1;
                    // SAFETY: a thread hotkey (no window); its WM_HOTKEY arrives in this thread's queue.
                    if unsafe { RegisterHotKey(None, id, modifiers(shortcut), vk) }.is_ok() {
                        self.registered.push(id);
                        registered[index] = true;
                    }
                }
            }
        }
        self.held_back = [false; 2];
        if self.mouse.is_empty() {
            if let Some(hook) = self.hook.take() {
                // SAFETY: the hook was installed by this thread.
                let _ = unsafe { UnhookWindowsHookEx(hook) };
            }
        } else if self.hook.is_none() {
            // SAFETY: the hook procedure lives in this executable and runs on this thread,
            // which keeps pumping messages.
            self.hook = unsafe {
                GetModuleHandleW(None)
                    .and_then(|module| SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(HINSTANCE(module.0)), 0))
            }
            .inspect_err(|err| log::warn!("could not watch the side mouse buttons: {err}"))
            .ok();
        }
        if self.hook.is_some() {
            for (index, _) in &self.mouse {
                registered[*index] = true;
            }
        }
        registered
    }
}

fn modifiers(shortcut: &Shortcut) -> HOT_KEY_MODIFIERS {
    let mut modifiers = MOD_NOREPEAT;
    for (on, flag) in [(shortcut.ctrl, MOD_CONTROL), (shortcut.alt, MOD_ALT), (shortcut.shift, MOD_SHIFT), (shortcut.win, MOD_WIN)] {
        if on {
            modifiers |= flag;
        }
    }
    modifiers
}

/// The virtual-key code of a `KeyboardEvent.code` name. Keys that type a character are named by
/// where they are, so they go through their scan code and the current keyboard layout: on AZERTY
/// the key named `KeyQ` is A.
fn virtual_key(code: &str) -> Option<u32> {
    if let Some(scan_code) = scan_code(code) {
        // SAFETY: a plain lookup in the keyboard layout.
        let vk = unsafe { MapVirtualKeyW(scan_code, MAPVK_VSC_TO_VK) };
        if vk != 0 {
            return Some(vk);
        }
    }
    let number = |prefix: &str| code.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok());
    if let Some(letter) = code.strip_prefix("Key").filter(|l| l.len() == 1) {
        return Some(u32::from(letter.as_bytes()[0]));
    }
    if let Some(digit) = number("Digit").filter(|d| *d <= 9) {
        return Some(0x30 + digit);
    }
    if let Some(digit) = number("Numpad").filter(|d| *d <= 9) {
        return Some(0x60 + digit);
    }
    if let Some(n) = number("F").filter(|n| (1..=24).contains(n)) {
        return Some(0x70 + n - 1);
    }
    Some(match code {
        "Space" => 0x20,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "End" => 0x23,
        "Home" => 0x24,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Insert" => 0x2D,
        "NumpadMultiply" => 0x6A,
        "NumpadAdd" => 0x6B,
        "NumpadSubtract" => 0x6D,
        "NumpadDecimal" => 0x6E,
        "NumpadDivide" => 0x6F,
        "Semicolon" => 0xBA,
        "Equal" => 0xBB,
        "Comma" => 0xBC,
        "Minus" => 0xBD,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Backquote" => 0xC0,
        "BracketLeft" => 0xDB,
        "Backslash" => 0xDC,
        "BracketRight" => 0xDD,
        "Quote" => 0xDE,
        "IntlBackslash" => 0xE2,
        _ => return None,
    })
}

/// Scan codes of the keys that type a character.
fn scan_code(code: &str) -> Option<u32> {
    const ROWS: [(&str, u32); 4] = [("1234567890", 0x02), ("QWERTYUIOP", 0x10), ("ASDFGHJKL", 0x1E), ("ZXCVBNM", 0x2C)];
    let name = code.strip_prefix("Key").or_else(|| code.strip_prefix("Digit")).filter(|n| n.len() == 1);
    if let Some(name) = name {
        return ROWS.iter().find_map(|(row, first)| row.find(name).map(|i| first + i as u32));
    }
    Some(match code {
        "Minus" => 0x0C,
        "Equal" => 0x0D,
        "BracketLeft" => 0x1A,
        "BracketRight" => 0x1B,
        "Semicolon" => 0x27,
        "Quote" => 0x28,
        "Backquote" => 0x29,
        "Backslash" => 0x2B,
        "Comma" => 0x33,
        "Period" => 0x34,
        "Slash" => 0x35,
        "IntlBackslash" => 0x56,
        _ => return None,
    })
}

fn is_down(vk: u16) -> bool {
    // SAFETY: a plain query of the key state.
    (unsafe { GetAsyncKeyState(i32::from(vk)) } as u16) & 0x8000 != 0
}

/// Holds back a side button press that is a shortcut, and its release, so the app under the
/// cursor does not also go back or forward. Must return quickly: Windows waits for it.
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let message = wparam.0 as u32;
    if code == HC_ACTION as i32 && (message == WM_XBUTTONDOWN || message == WM_XBUTTONUP) {
        // SAFETY: for these messages `lparam` points to an MSLLHOOKSTRUCT.
        let button = (unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).mouseData } >> 16) as u16;
        let key = match button {
            XBUTTON1 => Some((0, Key::MouseBack)),
            XBUTTON2 => Some((1, Key::MouseForward)),
            _ => None,
        };
        if let Some((slot, key)) = key {
            let pressed = LISTENER.with_borrow_mut(|listener| {
                let listener = listener.as_mut()?;
                if message == WM_XBUTTONUP {
                    return std::mem::take(&mut listener.held_back[slot]).then_some(None);
                }
                let (ctrl, alt, shift) = (is_down(VK_CONTROL.0), is_down(VK_MENU.0), is_down(VK_SHIFT.0));
                let win = is_down(VK_LWIN.0) || is_down(VK_RWIN.0);
                let (index, _) = listener.mouse.iter().find(|(_, s)| {
                    s.key == key && (s.ctrl, s.alt, s.shift, s.win) == (ctrl, alt, shift, win)
                })?;
                listener.held_back[slot] = true;
                Some(Some(*index))
            });
            if let Some(index) = pressed {
                if let Some(index) = index {
                    press(index);
                }
                return LRESULT(1);
            }
        }
    }
    // SAFETY: passes the event on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
