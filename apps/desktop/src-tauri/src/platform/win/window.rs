use std::sync::OnceLock;
use std::sync::atomic::{AtomicIsize, Ordering};

use tauri::WebviewWindow;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, HWND_TOPMOST, IsWindow, MA_NOACTIVATE,
    SetForegroundWindow, WA_INACTIVE, WM_ACTIVATE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_MOUSEACTIVATE,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

use crate::island::Rect;

const SUBCLASS_ID: usize = 0x4c5a_0001;

/// Makes the island a tool window that never takes focus: it stays out of Alt+Tab and the
/// taskbar, and clicking it leaves the user's current app active.
pub fn prepare_island_window(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?;
    unsafe {
        let set = (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0) as isize;
        let ex_style = (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) | set) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
        let _ = SetWindowSubclass(hwnd, Some(island_window_proc), SUBCLASS_ID, 0);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    Ok(())
}

/// Shows the window without activating it. Tauri's `show()` focuses the window, which would
/// steal focus from the app the user is working in.
pub fn show_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?;
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    Ok(())
}

/// Called when the island stops being the active window.
static ON_DEACTIVATED: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Calls `callback` whenever the island stops being the active window, for example when the user
/// clicks another app while typing in a widget. The island page cannot tell this by itself: its
/// own `blur` already fired when the focus moved into the widget's iframe.
pub fn on_deactivated(callback: impl Fn() + Send + Sync + 'static) {
    let _ = ON_DEACTIVATED.set(Box::new(callback));
}

/// The window that was active before the island took the keyboard, to give the focus back to.
static PREVIOUS_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

/// Lets the island receive keystrokes while a text field in it is in use: the window may be
/// activated again and comes to the foreground. The previously active window is remembered for
/// `release_keyboard`.
pub fn take_keyboard(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?;
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground != hwnd && !foreground.is_invalid() {
            PREVIOUS_FOREGROUND.store(foreground.0 as isize, Ordering::Relaxed);
        }
        set_no_activate(hwnd, false);
    }
    // Tauri falls back to a synthetic Alt press when Windows refuses the foreground change.
    window.set_focus()?;
    window.as_ref().set_focus()
}

/// Makes the island non-activating again. If it is still the active window, the focus goes back
/// to the window that was active before; if the user has already clicked elsewhere, it stays there.
pub fn release_keyboard(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?;
    let previous = HWND(PREVIOUS_FOREGROUND.swap(0, Ordering::Relaxed) as *mut _);
    unsafe {
        set_no_activate(hwnd, true);
        if GetForegroundWindow() == hwnd && !previous.is_invalid() && IsWindow(Some(previous)).as_bool() {
            let _ = SetForegroundWindow(previous);
        }
    }
    Ok(())
}

unsafe fn set_no_activate(hwnd: HWND, on: bool) {
    let flag = WS_EX_NOACTIVATE.0 as isize;
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, if on { ex_style | flag } else { ex_style & !flag });
    }
}

/// Clips the window to the given rectangles. Outside the region Windows routes mouse input to
/// the windows underneath, which gives per-shape click-through without polling the cursor.
pub fn set_hit_region(window: &WebviewWindow, rects: &[Rect], scale: f64) -> tauri::Result<()> {
    let hwnd = window.hwnd()?;
    unsafe {
        let region = CreateRectRgn(0, 0, 0, 0);
        for rect in rects {
            let part = CreateRectRgn(
                (rect.x * scale).floor() as i32,
                (rect.y * scale).floor() as i32,
                ((rect.x + rect.width) * scale).ceil() as i32,
                ((rect.y + rect.height) * scale).ceil() as i32,
            );
            CombineRgn(Some(region), Some(region), Some(part), RGN_OR);
            let _ = DeleteObject(part.into());
        }
        // On success the system owns the region, so it is only freed when the call fails.
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(region.into());
        }
    }
    Ok(())
}

unsafe extern "system" fn island_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if msg == WM_ACTIVATE && (wparam.0 & 0xffff) as u32 == WA_INACTIVE {
        if let Some(callback) = ON_DEACTIVATED.get() {
            callback();
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
