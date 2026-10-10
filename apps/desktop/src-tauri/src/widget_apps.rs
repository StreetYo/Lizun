//! Programs a widget may start (`apps` in its manifest, `apps.start` in the SDK), such as Anki
//! for the Anki widget, which talks to it through the AnkiConnect add-on.
//!
//! A widget names the program by an id from its manifest, never by a path, and the manifest
//! lists the paths it may be installed at; the user sees the program before installing the
//! widget. It is started without arguments. In the background, its windows are hidden as they
//! appear for a short while, so it runs without a window until the widget or the user shows it;
//! when Lizun quits, a program still in the background is asked to close. Why it works this way
//! is in docs/adr/0011-widgets-starting-apps.md.

use std::path::{Component, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

/// How long the windows of a program started in the background are hidden as they appear: long
/// enough for its start-up, short enough that opening it by hand later shows it as usual.
const HIDE_FOR: Duration = Duration::from_secs(30);
const MAX_APPS: usize = 4;
const MAX_PATHS: usize = 8;
const MAX_NAME_CHARS: usize = 60;
const MAX_PATH_CHARS: usize = 260;
/// Folders a path may start with, as `%NAME%`, and the environment variables they come from.
const FOLDERS: &[(&str, &str)] = &[
    ("%LOCALAPPDATA%", "LOCALAPPDATA"),
    ("%APPDATA%", "APPDATA"),
    ("%PROGRAMFILES%", "ProgramFiles"),
    ("%PROGRAMFILES(X86)%", "ProgramFiles(x86)"),
];

/// A program in a widget's manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSpec {
    /// What the widget calls it by.
    pub id: String,
    /// Shown to the user, such as `Anki`.
    pub name: String,
    /// Where the program may be installed, tried in order; the first that exists is used.
    pub paths: Vec<String>,
}

/// Programs started in the background in this session, by process id.
#[derive(Default)]
pub struct Started(Mutex<Vec<u32>>);

pub fn validate(apps: &[AppSpec]) -> Result<(), String> {
    if apps.len() > MAX_APPS {
        return Err(format!("more than {MAX_APPS} apps"));
    }
    for (i, app) in apps.iter().enumerate() {
        let id_ok = (1..=32).contains(&app.id.len())
            && app.id.starts_with(|c: char| c.is_ascii_lowercase())
            && app.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err(format!("app {:?}: the id is 1-32 lowercase letters, digits or dashes", app.id));
        }
        if apps[..i].iter().any(|other| other.id == app.id) {
            return Err(format!("app {:?} is listed twice", app.id));
        }
        if app.name.trim().is_empty() || app.name.chars().count() > MAX_NAME_CHARS {
            return Err(format!("app {:?}: the name is 1-{MAX_NAME_CHARS} characters", app.id));
        }
        if app.paths.is_empty() || app.paths.len() > MAX_PATHS {
            return Err(format!("app {:?}: list 1-{MAX_PATHS} paths", app.id));
        }
        if let Some(bad) = app.paths.iter().find(|path| !is_valid_path(path)) {
            return Err(format!(
                "app {:?}: invalid path {bad:?}: use C:\\...\\program.exe or start with %LOCALAPPDATA%, %APPDATA%, %PROGRAMFILES% or %PROGRAMFILES(X86)%",
                app.id
            ));
        }
    }
    Ok(())
}

/// A path to an `.exe` in a fixed folder: absolute with a drive letter or starting with one of
/// `FOLDERS`, with plain components only, so it cannot point anywhere by surprise.
fn is_valid_path(path: &str) -> bool {
    let rest = match FOLDERS.iter().find_map(|(name, _)| strip_prefix_ignore_case(path, name)) {
        Some(rest) => rest,
        None => {
            let bytes = path.as_bytes();
            let drive = bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
            if !drive {
                return false;
            }
            &path[2..]
        }
    };
    let Some(relative) = rest.strip_prefix('\\') else { return false };
    path.chars().count() <= MAX_PATH_CHARS
        && relative.to_ascii_lowercase().ends_with(".exe")
        && !relative.contains(['/', ':', '*', '?', '"', '<', '>', '|', '%'])
        && relative.split('\\').all(|part| !part.is_empty() && part != "." && part != "..")
}

fn strip_prefix_ignore_case<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let head = path.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &path[prefix.len()..])
}

/// The path with its folder variable filled in, if that variable is set.
fn expand(path: &str) -> Option<PathBuf> {
    for (name, variable) in FOLDERS {
        if let Some(rest) = strip_prefix_ignore_case(path, name) {
            let folder = std::env::var_os(variable)?;
            return Some(PathBuf::from(folder).join(rest.trim_start_matches('\\')));
        }
    }
    Some(PathBuf::from(path))
}

/// The first of the app's paths that exists.
fn locate(spec: &AppSpec) -> Option<PathBuf> {
    spec.paths
        .iter()
        .filter_map(|path| expand(path))
        .find(|path| path.components().all(|c| !matches!(c, Component::ParentDir)) && path.is_file())
}

fn spec_of<R: Runtime>(app: &AppHandle<R>, widget_id: &str, app_id: &str) -> Result<AppSpec, String> {
    let manifest = crate::widgets::find(app, widget_id).ok_or("unknown widget")?;
    manifest
        .apps
        .into_iter()
        .find(|spec| spec.id == app_id)
        .ok_or_else(|| format!("{app_id} is not in the apps list of manifest.json"))
}

/// Starts the program unless it is running already. Answers `started`, `running`, or
/// `notFound` when it is not installed at any of its paths.
#[tauri::command]
pub async fn widget_app_start(app: AppHandle, widget_id: String, app_id: String, hidden: bool) -> Result<&'static str, String> {
    let spec = spec_of(&app, &widget_id, &app_id)?;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(exe) = locate(&spec) else { return Ok("notFound") };
        if crate::platform::find_process(&exe).is_some() {
            return Ok("running");
        }
        log::info!("starting {} for {widget_id}{}", exe.display(), if hidden { " in the background" } else { "" });
        let pid = crate::platform::start_app(&exe, hidden.then_some(HIDE_FOR)).map_err(|err| err.to_string())?;
        if hidden {
            handle.state::<Started>().0.lock().unwrap_or_else(|e| e.into_inner()).push(pid);
        }
        Ok("started")
    })
    .await
    .map_err(|err| err.to_string())?
}

/// Shows the program's windows and brings it to the front. False when it is not running or has
/// no window to show.
#[tauri::command]
pub async fn widget_app_show(app: AppHandle, widget_id: String, app_id: String) -> Result<bool, String> {
    let spec = spec_of(&app, &widget_id, &app_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(pid) = locate(&spec).and_then(|exe| crate::platform::find_process(&exe)) else { return false };
        crate::platform::show_app(pid)
    })
    .await
    .map_err(|err| err.to_string())
}

/// When Lizun quits, programs it started in the background that are still hidden are asked to
/// close, so nothing keeps running unseen.
pub fn close_started<R: Runtime>(app: &AppHandle<R>) {
    let pids = std::mem::take(&mut *app.state::<Started>().0.lock().unwrap_or_else(|e| e.into_inner()));
    for pid in pids {
        if crate::platform::close_app_if_hidden(pid) {
            log::info!("asked the background app {pid} to close");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn spec(paths: &[&str]) -> AppSpec {
        AppSpec { id: "anki".into(), name: "Anki".into(), paths: paths.iter().map(|p| p.to_string()).collect() }
    }

    #[test]
    fn accepts_only_programs_in_fixed_folders() {
        for good in [
            r"%LOCALAPPDATA%\Programs\Anki\anki.exe",
            r"%ProgramFiles%\Anki\anki.exe",
            r"%PROGRAMFILES(X86)%\Anki\Anki.EXE",
            r"C:\Tools\app.exe",
        ] {
            assert!(validate(&[spec(&[good])]).is_ok(), "{good} should be accepted");
        }
        for bad in [
            r"anki.exe",
            r"%TEMP%\anki.exe",
            r"%LOCALAPPDATA%\..\x.exe",
            r"%LOCALAPPDATA%\Programs\Anki\anki.exe --flag",
            r"%LOCALAPPDATA%\Programs\Anki\anki.bat",
            r"%LOCALAPPDATA%Programs\anki.exe",
            r"C:/Tools/app.exe",
            r"\\server\share\app.exe",
            r"C:\Tools\%PATH%\app.exe",
            r"C:\Tools\.\app.exe",
        ] {
            assert!(validate(&[spec(&[bad])]).is_err(), "{bad} should be rejected");
        }
        assert!(validate(&[spec(&[])]).is_err());
        let mut twice = vec![spec(&[r"C:\a.exe"]), spec(&[r"C:\b.exe"])];
        assert!(validate(&twice).is_err());
        twice[1].id = "Bad Id".into();
        assert!(validate(&twice).is_err());
    }

    #[test]
    fn expands_folder_variables() {
        let local = std::env::var("LOCALAPPDATA").unwrap();
        assert_eq!(
            expand(r"%localappdata%\Programs\Anki\anki.exe").unwrap(),
            Path::new(&local).join(r"Programs\Anki\anki.exe")
        );
        assert_eq!(expand(r"C:\Tools\app.exe").unwrap(), PathBuf::from(r"C:\Tools\app.exe"));
        assert!(locate(&spec(&[r"C:\no\such\program.exe"])).is_none());
    }
}
