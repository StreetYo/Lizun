mod clipboard;
mod hotkeys;
mod media;
mod window;

use std::path::Path;

pub use clipboard::{
    clipboard_sequence, read_clipboard, write_clipboard_files, write_clipboard_picture, write_clipboard_text,
};
pub use hotkeys::{HotkeyListener, spawn_hotkey_listener};
pub use media::{MediaController, spawn_media_watcher};
pub use window::{
    on_deactivated, prepare_island_window, release_keyboard, set_hit_region, show_without_activation, take_keyboard,
};

/// Opens the folder in Explorer.
pub fn reveal_folder(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer").arg(path).spawn().map(|_| ())
}

/// The IANA name of the time zone set in Windows right now. Read anew on every call, so a
/// change in the system settings shows up without a restart. `None` when the zone has no IANA
/// name or daylight saving is switched off for it, which no IANA zone describes.
pub fn current_time_zone() -> Option<String> {
    use windows::Win32::System::Time::{DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation, TIME_ZONE_ID_INVALID};

    let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // SAFETY: `info` is a valid, writable structure of the expected type.
    if unsafe { GetDynamicTimeZoneInformation(&mut info) } == TIME_ZONE_ID_INVALID || info.DynamicDaylightTimeDisabled {
        return None;
    }
    let key = &info.TimeZoneKeyName;
    let key = String::from_utf16_lossy(&key[..key.iter().position(|&c| c == 0).unwrap_or(key.len())]);
    iana_time_zone(&key)
}

/// The IANA name of a Windows time zone such as "Russian Standard Time", from the ICU library
/// that ships with Windows.
pub fn iana_time_zone(windows_name: &str) -> Option<String> {
    use windows::Win32::Globalization::{U_ZERO_ERROR, ucal_getTimeZoneIDForWindowsID};
    use windows::core::PCSTR;

    let name: Vec<u16> = windows_name.encode_utf16().collect();
    let mut id = [0u16; 64];
    let mut status = U_ZERO_ERROR;
    // SAFETY: both buffers are valid for the lengths passed; a null region means the default one.
    let length = unsafe {
        ucal_getTimeZoneIDForWindowsID(
            name.as_ptr(),
            i32::try_from(name.len()).ok()?,
            PCSTR::null(),
            id.as_mut_ptr(),
            id.len() as i32,
            &mut status,
        )
    };
    // ICU reports errors as positive codes and warnings as negative ones.
    let length = usize::try_from(length).ok().filter(|&length| status.0 <= 0 && (1..=id.len()).contains(&length))?;
    Some(String::from_utf16_lossy(&id[..length]))
}

/// The user's Windows display languages, most preferred first, as tags such as `ru-RU`.
pub fn preferred_languages() -> Vec<String> {
    use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    use windows::core::PWSTR;

    let (mut count, mut length) = (0u32, 0u32);
    // SAFETY: without a buffer the call only reports the length it needs.
    if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut length) }.is_err() {
        return Vec::new();
    }
    let mut tags = vec![0u16; length as usize];
    // SAFETY: the buffer holds the `length` characters asked for above.
    let filled = unsafe {
        GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, Some(PWSTR(tags.as_mut_ptr())), &mut length)
    };
    if filled.is_err() {
        return Vec::new();
    }
    // A list of tags, each ending in a null, with one more null after the last.
    tags.split(|&c| c == 0).filter(|tag| !tag.is_empty()).map(String::from_utf16_lossy).collect()
}
