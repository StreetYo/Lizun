//! Updates Lizun in the background. A new version announced in `latest.json` on the website is
//! downloaded as soon as it is found and installed once the user has left the computer alone
//! for a while: the setup program runs without a window and starts Lizun again, so the island
//! is gone for a few seconds while nobody is looking. `autoUpdate` in the preferences turns
//! all of it off, including the checks.
//!
//! Only copies installed by the setup program update themselves. The MSI is for managed
//! installs, where updates are the administrator's business, and the portable copy and
//! development builds have no setup program to run.

use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// The first check waits until the computer has settled after signing in.
const FIRST_CHECK: Duration = Duration::from_secs(2 * 60);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// A check that failed, offline for example, is tried again sooner.
const RETRY_AFTER: Duration = Duration::from_secs(30 * 60);
/// A downloaded update is installed once the mouse and keyboard have been left alone this long.
const IDLE_BEFORE_INSTALL: Duration = Duration::from_secs(5 * 60);
const TICK: Duration = Duration::from_secs(30);

pub fn start(app: &AppHandle) {
    if !installed_by_setup() {
        log::info!("automatic updates are off: this copy was not installed by the setup program");
        return;
    }
    let app = app.clone();
    let started = std::thread::Builder::new().name("updater".into()).spawn(move || run(&app));
    if let Err(err) = started {
        log::warn!("could not start the updater: {err}");
    }
}

/// The NSIS setup program leaves its uninstaller next to the program; the MSI and the portable
/// zip do not.
fn installed_by_setup() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("uninstall.exe").is_file()))
        .unwrap_or(false)
}

fn enabled(app: &AppHandle) -> bool {
    crate::preferences::current(app).auto_update
}

fn run(app: &AppHandle) {
    let mut next_check = Instant::now() + FIRST_CHECK;
    let mut downloaded: Option<(Update, Vec<u8>)> = None;
    loop {
        std::thread::sleep(TICK);
        if !enabled(app) {
            continue;
        }
        if let Some((update, bytes)) = &downloaded {
            if crate::platform::idle_time().is_some_and(|idle| idle >= IDLE_BEFORE_INSTALL) {
                log::info!("installing Lizun {}", update.version);
                // Runs the setup program and ends this process; it only returns on failure.
                if let Err(err) = update.install(bytes) {
                    log::warn!("could not install Lizun {}: {err}", update.version);
                    downloaded = None;
                    next_check = Instant::now() + RETRY_AFTER;
                }
            }
            continue;
        }
        if Instant::now() < next_check {
            continue;
        }
        match tauri::async_runtime::block_on(fetch(app)) {
            Ok(update) => {
                downloaded = update;
                next_check = Instant::now() + CHECK_EVERY;
            }
            Err(err) => {
                log::warn!("could not check for updates: {err}");
                next_check = Instant::now() + RETRY_AFTER;
            }
        }
    }
}

/// Downloads the newer version, if there is one. The signature is checked on download.
async fn fetch(app: &AppHandle) -> tauri_plugin_updater::Result<Option<(Update, Vec<u8>)>> {
    let closing = app.clone();
    let updater = app
        .updater_builder()
        // In place of the plugin's own hook, which skips the RunEvent::Exit handler in lib.rs.
        .on_before_exit(move || {
            crate::widget_apps::close_started(&closing);
            closing.cleanup_before_exit();
        })
        .build()?;
    let Some(update) = updater.check().await? else {
        log::info!("Lizun is up to date");
        return Ok(None);
    };
    log::info!("downloading Lizun {}", update.version);
    let bytes = update.download(|_, _| {}, || {}).await?;
    log::info!("Lizun {} is downloaded and waits for the computer to be idle", update.version);
    Ok(Some((update, bytes)))
}
