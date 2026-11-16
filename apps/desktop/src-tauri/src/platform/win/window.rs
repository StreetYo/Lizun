use std::sync::OnceLock;
use std::sync::atomic::{AtomicIsize, Ordering};

use tauri::WebviewWindow;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromWindow,
    RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, GW_HWNDPREV, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetShellWindow, GetWindow,
    GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, IsWindow, IsWindowVisible, IsZoomed, MA_NOACTIVATE, STYLESTRUCT,
    SetForegroundWindow, SetTimer, WA_INACTIVE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_ACTIVATE,
    WM_DISPLAYCHANGE, WM_STYLECHANGING, WM_TIMER, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_MOUSEACTIVATE, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

use crate::island::Rect;

const SUBCLASS_ID: usize = 0x4c5a_0001;
/// Timer that checks the island is still above ordinary windows, and how often it runs.
const ON_TOP_TIMER: usize = 0x4c5a_0002;
const ON_TOP_CHECK_MS: u32 = 1000;
/// Most windows looked at above the island: a window list that changes during the walk could
/// otherwise keep it going.
const MAX_WINDOWS_ABOVE: usize = 1024;

/// The island window, for the foreground hook.
static ISLAND: AtomicIsize = AtomicIsize::new(0);

/// Makes the island a tool window that never takes focus: it stays out of Alt+Tab and the
/// taskbar, and clicking it leaves the user's current app active. Must be called on the main
/// thread, whose message loop delivers the foreground hook.
pub fn prepare_island_window(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?;
    ISLAND.store(hwnd.0 as isize, Ordering::Relaxed);
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
        // Windows can leave a topmost window below ordinary ones while keeping its topmost flag,
        // and nothing would bring the island back: it would no longer open when pointed at. It
        // is checked whenever another app becomes active, and once a second in case it was
        // pushed down at another moment.
        let _ = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        SetTimer(Some(hwnd), ON_TOP_TIMER, ON_TOP_CHECK_MS, None);
    }
    Ok(())
}

/// Shows the window without activating it. Tauri's `show()` focuses the window, which would
/// steal focus from the app the user is working in.
pub fn show_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?;
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        bring_to_top(hwnd);
    }
    Ok(())
}

unsafe fn bring_to_top(hwnd: HWND) {
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
    }
}

unsafe extern "system" fn on_foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _foreground: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let island = HWND(ISLAND.load(Ordering::Relaxed) as *mut _);
    if !island.is_invalid() {
        unsafe { keep_on_top(island) };
    }
}

/// Puts the shown island back on top if an ordinary window has ended up above it, unless the
/// active window is fullscreen on the island's monitor.
unsafe fn keep_on_top(island: HWND) {
    unsafe {
        if IsWindowVisible(island).as_bool()
            && is_buried(island)
            && !is_fullscreen_over(GetForegroundWindow(), island)
        {
            bring_to_top(island);
        }
    }
}

/// Whether a shown window without the topmost flag is above the island. Topmost windows stay
/// above all others, so this is the state Windows sometimes leaves the island in.
unsafe fn is_buried(island: HWND) -> bool {
    let mut window = island;
    for _ in 0..MAX_WINDOWS_ABOVE {
        unsafe {
            let Ok(above) = GetWindow(window, GW_HWNDPREV) else { return false };
            let topmost = GetWindowLongPtrW(above, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0;
            if IsWindowVisible(above).as_bool() && !topmost {
                return true;
            }
            window = above;
        }
    }
    false
}

/// Whether `window` covers the whole monitor the island is on, like a game, a slideshow or a
/// video in a browser. The island does not climb over it. The desktop also covers the monitor
/// but does not count, and neither does a maximized window, whose frame reaches past the edges.
unsafe fn is_fullscreen_over(window: HWND, island: HWND) -> bool {
    unsafe {
        if window.is_invalid() || window == GetShellWindow() || IsZoomed(window).as_bool() {
            return false;
        }
        let mut class = [0u16; 16];
        let len = GetClassNameW(window, &mut class) as usize;
        if matches!(String::from_utf16_lossy(&class[..len]).as_str(), "WorkerW" | "Progman") {
            return false;
        }
        let monitor = MonitorFromWindow(island, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() || MonitorFromWindow(window, MONITOR_DEFAULTTONULL) != monitor {
            return false;
        }
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let mut rect = RECT::default();
        GetMonitorInfoW(monitor, &mut info).as_bool()
            && GetWindowRect(window, &mut rect).is_ok()
            && rect.left <= info.rcMonitor.left
            && rect.top <= info.rcMonitor.top
            && rect.right >= info.rcMonitor.right
            && rect.bottom >= info.rcMonitor.bottom
    }
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
    if msg == WM_TIMER && wparam.0 == ON_TOP_TIMER {
        unsafe { keep_on_top(hwnd) };
        return LRESULT(0);
    }
    // Tao rebuilds the extended style from its own flags on hide, show and other state changes,
    // which would turn the island into an ordinary app window. It stays a tool window.
    if msg == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 {
        let styles = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
        styles.styleNew = (styles.styleNew | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
    }
    if msg == WM_ACTIVATE
        && (wparam.0 & 0xffff) as u32 == WA_INACTIVE
        && let Some(callback) = ON_DEACTIVATED.get()
    {
        callback();
    }
    // A monitor was added or removed, or changed its resolution or which one is primary.
    if msg == WM_DISPLAYCHANGE {
        crate::island::displays_changed();
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
