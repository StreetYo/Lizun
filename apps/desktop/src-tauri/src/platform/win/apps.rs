//! Starting a program for a widget, optionally in the background: its windows are hidden as
//! they appear during the first moments, so it runs without a window or a taskbar button until
//! it is shown. A running program can be put in the background and shown again; while it is
//! there, starting it again (from its shortcut, say) shows it (see `widget_apps.rs`).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
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
    EVENT_OBJECT_SHOW, EnumWindows, GA_ROOT, GA_ROOTOWNER, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetMessageW, GetWindow,
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

/// Programs in the background that are shown when started again, by process id.
static WATCHING: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// Windows `hide_app` hid besides the main ones, such as an open dialog: process id and window.
static HIDDEN_DIALOGS: Mutex<Vec<(u32, isize)>> = Mutex::new(Vec::new());
/// How often a program in the background is checked for being started again.
const REOPEN_CHECK: Duration = Duration::from_millis(200);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// The id of a running process started from `exe`, if any.
pub fn find_process(exe: &Path) -> Option<u32> {
    processes_of(exe).into_iter().next()
}

/// The running processes started from `exe`. Only processes with its file name are opened to
/// compare the whole path, so this is cheap enough to run a few times a second.
fn processes_of(exe: &Path) -> Vec<u32> {
    let wanted = exe.to_string_lossy().to_lowercase();
    let file_name = exe.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    let mut found = Vec::new();
    // SAFETY: the snapshot handle is closed below; the entry is a valid, sized structure.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return found };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let length = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..length]).to_lowercase() == file_name
                && image_path(entry.th32ProcessID).is_some_and(|path| path.to_lowercase() == wanted)
            {
                found.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    found
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
        watch_for_reopen(pid, exe.to_path_buf());
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

/// Shows the program's main windows and the dialogs `hide_app` hid, restores them if minimized
/// and brings them to the front; it is no longer kept in the background. False when it has no
/// main window.
pub fn show_app(pid: u32) -> bool {
    stop_hiding(pid);
    stop_watching(pid);
    let windows = main_windows(pid);
    let dialogs: Vec<HWND> = {
        let mut hidden = lock(&HIDDEN_DIALOGS);
        let dialogs = hidden.iter().filter(|(owner, _)| *owner == pid).map(|&(_, hwnd)| HWND(hwnd as _)).collect();
        hidden.retain(|(owner, _)| *owner != pid);
        dialogs
    };
    // SAFETY: the handles were just enumerated or recorded; a window closed meanwhile makes the
    // calls fail.
    unsafe {
        for &hwnd in windows.iter().chain(&dialogs) {
            if !IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }
        // A dialog goes on top of its window, as before it was hidden.
        if let Some(&hwnd) = dialogs.last().or(windows.first()) {
            // Allowed when Lizun received the click that asked for it; otherwise Windows flashes
            // the taskbar button instead.
            let _ = SetForegroundWindow(hwnd);
        }
    }
    !windows.is_empty()
}

/// Puts the program in the background: hides its main windows and the windows they own, such as
/// an open dialog, so it has no window or taskbar button until `show_app`, or until it is started
/// again. False when it has no main window.
pub fn hide_app(pid: u32, exe: &Path) -> bool {
    let windows = main_windows(pid);
    if windows.is_empty() {
        return false;
    }
    let mut dialogs = Vec::new();
    // SAFETY: the handles were just enumerated; a window closing meanwhile makes the calls fail.
    unsafe {
        for hwnd in top_level_windows(pid) {
            if windows.contains(&hwnd) || !IsWindowVisible(hwnd).as_bool() {
                continue;
            }
            if windows.contains(&GetAncestor(hwnd, GA_ROOTOWNER)) {
                let _ = ShowWindow(hwnd, SW_HIDE);
                if is_framed(hwnd) {
                    dialogs.push((pid, hwnd.0 as isize));
                }
            }
        }
        for &hwnd in &windows {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    let mut hidden = lock(&HIDDEN_DIALOGS);
    hidden.retain(|(owner, _)| *owner != pid);
    hidden.extend(dialogs);
    drop(hidden);
    watch_for_reopen(pid, exe.to_path_buf());
    true
}

/// Whether one of the program's main windows is shown, minimized or not.
pub fn app_shown(pid: u32) -> bool {
    // SAFETY: plain queries on window handles that were just enumerated.
    main_windows(pid).into_iter().any(|hwnd| unsafe { IsWindowVisible(hwnd).as_bool() })
}

/// While the program is in the background, watches for it being started again, from its shortcut
/// for example, and shows it then. Anki, like many programs, hands such a start to the running
/// instance and asks it to raise its window, which does nothing for a window hidden from outside:
/// the program still takes it for shown. The second instance lives about half a second, so a few
/// checks a second catch it.
fn watch_for_reopen(pid: u32, exe: PathBuf) {
    {
        let mut watching = lock(&WATCHING);
        if watching.contains(&pid) {
            return;
        }
        watching.push(pid);
    }
    let spawned = std::thread::Builder::new().name("lizun-app-reopen".into()).spawn(move || {
        loop {
            std::thread::sleep(REOPEN_CHECK);
            if !lock(&WATCHING).contains(&pid) {
                return;
            }
            if !is_running(pid) {
                stop_watching(pid);
                return;
            }
            if processes_of(&exe).into_iter().any(|other| other != pid) {
                log::info!("{} was started again, showing it", exe.display());
                show_app(pid);
                return;
            }
        }
    });
    if spawned.is_err() {
        stop_watching(pid);
    }
}

fn stop_watching(pid: u32) {
    lock(&WATCHING).retain(|&watched| watched != pid);
}

/// Asks a program started in the background to close, the way its close button would, unless
/// one of its main windows has been shown since. True when it was asked.
pub fn close_app_if_hidden(pid: u32) -> bool {
    stop_hiding(pid);
    stop_watching(pid);
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
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                let helper = ex_style & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0;
                let unowned = GetWindow(hwnd, GW_OWNER).map_or(true, |owner| owner.is_invalid());
                is_framed(hwnd) && !helper && unowned && GetWindowTextLengthW(hwnd) > 0
            }
        })
        .collect()
}

/// A window with a title bar and a window menu, as main windows and dialogs have.
fn is_framed(hwnd: HWND) -> bool {
    // SAFETY: a plain query on a window handle.
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    style & WS_CAPTION.0 == WS_CAPTION.0 && style & WS_SYSMENU.0 != 0
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
