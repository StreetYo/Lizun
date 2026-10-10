//! Starting a program for a widget, optionally in the background: its windows are hidden as
//! they appear during the first moments, so it runs without a window or a taskbar button until
//! it is shown (see `widget_apps.rs`).

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WAIT_TIMEOUT, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CreateProcessW, GetCurrentThreadId, OpenProcess, PROCESS_INFORMATION, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, ResumeThread,
    STARTF_USESHOWWINDOW, STARTUPINFOW, WaitForSingleObject,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_SHOW, EnumWindows, GA_ROOT, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetMessageW, GetWindow,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, MSG, PostMessageW,
    PostThreadMessageW, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL, SetForegroundWindow, SetTimer, ShowWindow,
    WINEVENT_OUTOFCONTEXT, WM_CLOSE, WM_QUIT, WM_TIMER, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_SYSMENU,
};
use windows::core::{BOOL, PCWSTR, PWSTR};

/// Programs whose windows are being hidden as they appear, by process id: the thread that runs
/// the hook.
static HIDING: Mutex<Option<HashMap<u32, u32>>> = Mutex::new(None);

fn with_hiding<T>(f: impl FnOnce(&mut HashMap<u32, u32>) -> T) -> T {
    let mut hiding = HIDING.lock().unwrap_or_else(|e| e.into_inner());
    f(hiding.get_or_insert_with(HashMap::new))
}

/// The id of a running process started from `exe`, if any.
pub fn find_process(exe: &Path) -> Option<u32> {
    let wanted = exe.to_string_lossy().to_lowercase();
    // SAFETY: the snapshot handle is closed below; the entry is a valid, sized structure.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = None;
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            if image_path(entry.th32ProcessID).is_some_and(|path| path.to_lowercase() == wanted) {
                found = Some(entry.th32ProcessID);
                break;
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

fn image_path(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed before returning; the buffer length is passed along.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length);
        let _ = CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..length as usize]))
    }
}

/// Whether the process is still running.
fn is_running(pid: u32) -> bool {
    // SAFETY: the handle is closed before returning.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else { return false };
        let running = WaitForSingleObject(process, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(process);
        running
    }
}

/// Starts `exe` without arguments in its own folder and returns the process id. With
/// `hide_for`, every window it shows during that time is hidden at once.
pub fn start_app(exe: &Path, hide_for: Option<Duration>) -> io::Result<u32> {
    let application: Vec<u16> = exe.as_os_str().encode_wide_null();
    // The command line holds only the quoted program, as if started from a shortcut.
    let mut command_line: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain([0]).collect();
    let folder: Vec<u16> = exe.parent().unwrap_or(Path::new(".")).as_os_str().encode_wide_null();
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW,
        // Applies to the program's first window; the hook below takes care of the rest.
        wShowWindow: if hide_for.is_some() { SW_HIDE.0 as u16 } else { SW_SHOWNORMAL.0 as u16 },
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: all strings are NUL-terminated and outlive the call; the structures are valid.
    unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_SUSPENDED,
            None,
            PCWSTR(folder.as_ptr()),
            &startup,
            &mut process,
        )
        .map_err(io::Error::other)?;
    }
    let pid = process.dwProcessId;
    // The process waits, suspended, until its windows can be caught from the very first one.
    if let Some(duration) = hide_for {
        if let Err(err) = start_hiding(pid, duration) {
            log::warn!("cannot hide the windows of {}: {err}", exe.display());
        }
    }
    // SAFETY: both handles come from CreateProcessW and are closed once.
    unsafe {
        ResumeThread(process.hThread);
        let _ = CloseHandle(process.hThread);
        let _ = CloseHandle(process.hProcess);
    }
    Ok(pid)
}

trait EncodeWideNull {
    fn encode_wide_null(&self) -> Vec<u16>;
}

impl EncodeWideNull for std::ffi::OsStr {
    fn encode_wide_null(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain([0]).collect()
    }
}

/// Runs a thread with a hook that hides the process's windows as they are shown, until
/// `duration` has passed or `show_app` is called.
fn start_hiding(pid: u32, duration: Duration) -> io::Result<()> {
    let (ready, installed) = mpsc::channel();
    std::thread::Builder::new().name("lizun-app-hider".into()).spawn(move || {
        // SAFETY: the hook and the timer belong to this thread, which pumps messages until done.
        unsafe {
            let hook = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                None,
                Some(on_window_shown),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            with_hiding(|hiding| hiding.insert(pid, GetCurrentThreadId()));
            let _ = ready.send(!hook.is_invalid());
            SetTimer(None, 0, duration.as_millis().min(u32::MAX as u128) as u32, None);
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                if message.message == WM_TIMER {
                    break;
                }
            }
            let _ = UnhookWinEvent(hook);
            with_hiding(|hiding| hiding.remove(&pid));
        }
    })?;
    match installed.recv_timeout(Duration::from_secs(2)) {
        Ok(true) => Ok(()),
        _ => Err(io::Error::other("the window hook could not be installed")),
    }
}

unsafe extern "system" fn on_window_shown(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    // Only whole top-level windows, not controls or parts of them.
    if object != 0 || child != 0 || hwnd.is_invalid() {
        return;
    }
    // SAFETY: plain queries and a ShowWindow on a window handle the system just reported.
    unsafe {
        if GetAncestor(hwnd, GA_ROOT) != hwnd {
            return;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if with_hiding(|hiding| hiding.contains_key(&pid)) {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Stops hiding the program's windows as they appear.
fn stop_hiding(pid: u32) {
    if let Some(thread) = with_hiding(|hiding| hiding.remove(&pid)) {
        // SAFETY: posting to a thread id is harmless if the thread has already ended.
        unsafe {
            let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

/// Shows the program's main windows, restores them if minimized and brings the first to the
/// front; it is no longer kept in the background. False when it has no main window.
pub fn show_app(pid: u32) -> bool {
    stop_hiding(pid);
    let windows = main_windows(pid);
    // SAFETY: the handles were just enumerated; a window closing meanwhile makes the calls fail.
    unsafe {
        for &hwnd in &windows {
            if !IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }
        if let Some(&hwnd) = windows.first() {
            // Allowed: Lizun received the click that asked for it.
            let _ = SetForegroundWindow(hwnd);
        }
    }
    !windows.is_empty()
}

/// Asks a program started in the background to close, the way its close button would, unless
/// one of its main windows has been shown since. True when it was asked.
pub fn close_app_if_hidden(pid: u32) -> bool {
    stop_hiding(pid);
    if !is_running(pid) {
        return false;
    }
    let windows = main_windows(pid);
    // SAFETY: the handles were just enumerated; posting to a closed window fails harmlessly.
    unsafe {
        if windows.iter().any(|&hwnd| IsWindowVisible(hwnd).as_bool()) {
            return false;
        }
        windows.iter().fold(false, |asked, &hwnd| asked | PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok())
    }
}

/// The windows a user would see as the program: top-level, without an owner, with a title bar,
/// a window menu and a title, and not a tool or non-activating window. Frameworks keep helper
/// windows that pass some of these tests (Qt's title bar and screen observers, Chromium's
/// widgets), so all of them are needed.
fn main_windows(pid: u32) -> Vec<HWND> {
    top_level_windows(pid)
        .into_iter()
        .filter(|&hwnd| {
            // SAFETY: plain queries on a window handle.
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                let framed = style & WS_CAPTION.0 == WS_CAPTION.0 && style & WS_SYSMENU.0 != 0;
                let helper = ex_style & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0;
                let unowned = GetWindow(hwnd, GW_OWNER).map_or(true, |owner| owner.is_invalid());
                framed && !helper && unowned && GetWindowTextLengthW(hwnd) > 0
            }
        })
        .collect()
}

fn top_level_windows(pid: u32) -> Vec<HWND> {
    struct Search {
        pid: u32,
        found: Vec<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, search: LPARAM) -> BOOL {
        // SAFETY: `search` points to the `Search` below for the duration of EnumWindows.
        unsafe {
            let search = &mut *(search.0 as *mut Search);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == search.pid {
                search.found.push(hwnd);
            }
        }
        true.into()
    }
    let mut search = Search { pid, found: Vec::new() };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}
