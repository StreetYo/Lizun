//! Now-playing state shared by the island UI and widgets. The platform backend watches the
//! system media session and pushes updates here; the UI reads them through events and commands.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT_CHANGED: &str = "media://changed";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// Identifier of the app that owns the session (an AppUserModelID on Windows).
    pub source_app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    /// Unix time in milliseconds at which `position_ms` was sampled, so the UI can advance
    /// the progress bar on its own while playing.
    pub position_sampled_at: Option<u64>,
    /// Changes whenever the artwork image changes; fetch the image with `media_artwork`.
    pub artwork_key: Option<String>,
    pub can_toggle: bool,
    pub can_next: bool,
    pub can_previous: bool,
}

#[derive(Debug, Clone)]
pub struct Artwork {
    pub key: String,
    pub data_url: String,
}

pub enum MediaUpdate {
    NowPlaying(Option<NowPlaying>),
    Artwork(Artwork),
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
}

#[derive(Default)]
pub struct MediaState {
    now_playing: Mutex<Option<NowPlaying>>,
    artwork: Mutex<Option<Artwork>>,
    controller: Mutex<Option<crate::platform::MediaController>>,
}

pub fn start(app: &AppHandle) {
    app.manage(MediaState::default());
    let handle = app.clone();
    let controller = crate::platform::spawn_media_watcher(move |update| {
        let state = handle.state::<MediaState>();
        match update {
            // The backend always delivers the artwork before the update that references it.
            MediaUpdate::Artwork(artwork) => *state.artwork.lock().unwrap() = Some(artwork),
            MediaUpdate::NowPlaying(now_playing) => {
                *state.now_playing.lock().unwrap() = now_playing.clone();
                if let Err(err) = handle.emit(EVENT_CHANGED, now_playing) {
                    log::warn!("failed to emit media update: {err}");
                }
            }
        }
    });
    *app.state::<MediaState>().controller.lock().unwrap() = controller;
}

#[tauri::command]
pub fn media_snapshot(state: State<'_, MediaState>) -> Option<NowPlaying> {
    state.now_playing.lock().unwrap().clone()
}

#[tauri::command]
pub fn media_artwork(state: State<'_, MediaState>, key: String) -> Option<String> {
    let artwork = state.artwork.lock().unwrap();
    artwork.as_ref().filter(|a| a.key == key).map(|a| a.data_url.clone())
}

#[tauri::command]
pub fn media_control(state: State<'_, MediaState>, command: MediaCommand) -> Result<(), String> {
    let controller = state.controller.lock().unwrap();
    let controller = controller.as_ref().ok_or("media backend is unavailable")?;
    controller.send(command)
}
