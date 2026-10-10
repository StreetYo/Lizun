//! Media sessions via Global System Media Transport Controls (SMTC): every app that shows up in
//! the Windows media flyout (Spotify, browsers, Apple Music, ...) is visible here.
//!
//! Windows' own "current session" is not used: it sticks to sessions that stopped long ago (a
//! voice message Telegram played days earlier) while another app plays. Every session is
//! watched instead, and `media::update` picks the one to show.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

use crate::media::{Artwork, MediaCommand, NowPlaying, Player};

/// Artwork larger than this is ignored; real covers are a few hundred kilobytes at most.
const MAX_ARTWORK_BYTES: u32 = 8 * 1024 * 1024;

/// Offset between the Windows epoch (1601) and the Unix epoch, in 100 ns ticks.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

/// How often the session list is compared with Windows', in case a change event was missed.
const RESYNC_INTERVAL: Duration = Duration::from_secs(5);

/// Wait before asking Windows for the sessions again after it failed; doubled on every failure
/// in a row, up to `RETRY_MAX`.
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);

enum Message {
    SessionsChanged,
    PropertiesChanged(String),
    PlaybackChanged,
    Command(String, MediaCommand),
}

pub struct MediaController {
    tx: Sender<Message>,
}

impl MediaController {
    /// Sends `command` to the session that `media::Player::id` names.
    pub fn send(&self, player_id: &str, command: MediaCommand) -> Result<(), String> {
        self.tx
            .send(Message::Command(player_id.to_string(), command))
            .map_err(|_| "the media thread has stopped".to_string())
    }
}

pub fn spawn_media_watcher(on_update: impl Fn(Vec<Player>) + Send + 'static) -> Option<MediaController> {
    let (tx, rx) = mpsc::channel();
    let thread_tx = tx.clone();
    let spawned = thread::Builder::new().name("lizun-media".into()).spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        run(thread_tx, rx, on_update);
    });
    match spawned {
        Ok(_) => Some(MediaController { tx }),
        Err(err) => {
            log::warn!("could not start the media thread: {err}");
            None
        }
    }
}

/// Watches the sessions for as long as the app runs. The session manager fails for good when
/// the shell goes away (explorer restarting, or not ready yet at autostart), so it is asked for
/// again, with a growing wait in between; meanwhile no player is shown.
fn run(tx: Sender<Message>, rx: Receiver<Message>, on_update: impl Fn(Vec<Player>)) {
    let mut last: Vec<Player> = Vec::new();
    let mut retry = RETRY_MIN;
    loop {
        let started = Instant::now();
        let Err(err) = watch(&tx, &rx, &on_update, &mut last) else { return };
        if started.elapsed() > RETRY_MAX {
            retry = RETRY_MIN;
        }
        log::warn!("media sessions unavailable, trying again in {retry:?}: {err}");
        if !last.is_empty() {
            last.clear();
            on_update(Vec::new());
        }
        if !wait_to_retry(&rx, retry) {
            return;
        }
        retry = (retry * 2).min(RETRY_MAX);
    }
}

/// Waits `delay`; commands that come meanwhile have no session to go to. False once the
/// controller is gone.
fn wait_to_retry(rx: &Receiver<Message>, delay: Duration) -> bool {
    let until = Instant::now() + delay;
    loop {
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(Message::Command(key, command)) => log::debug!("media command {command:?} while reconnecting: {key}"),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return true,
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

/// One session manager, until it fails (`Err`) or the controller is gone (`Ok`). Its handlers
/// are removed either way, so a failed manager does not keep posting.
fn watch(
    tx: &Sender<Message>,
    rx: &Receiver<Message>,
    on_update: &impl Fn(Vec<Player>),
    last: &mut Vec<Player>,
) -> windows::core::Result<()> {
    let manager = SessionManager::RequestAsync()?.join()?;
    let sessions_tx = tx.clone();
    let token = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        let _ = sessions_tx.send(Message::SessionsChanged);
        Ok(())
    }))?;
    let mut watched: Vec<WatchedSession> = Vec::new();
    let result = follow(&manager, tx, rx, on_update, last, &mut watched);
    for session in watched {
        session.unsubscribe();
    }
    let _ = manager.RemoveSessionsChanged(token);
    result
}

/// All WinRT calls happen on this thread. Event callbacks only post messages back to it, and
/// bursts of messages are coalesced so a track change produces one update. Only a failure of
/// the manager itself ends this; a session that fails is left out.
fn follow(
    manager: &SessionManager,
    tx: &Sender<Message>,
    rx: &Receiver<Message>,
    on_update: &impl Fn(Vec<Player>),
    last: &mut Vec<Player>,
    watched: &mut Vec<WatchedSession>,
) -> windows::core::Result<()> {
    let _ = tx.send(Message::SessionsChanged);

    loop {
        let first = match rx.recv_timeout(RESYNC_INTERVAL) {
            Ok(message) => Some(message),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let mut sessions_changed = first.is_none() && session_keys(manager)? != keys(watched);
        let mut properties_changed = HashSet::new();
        let mut commands = Vec::new();
        for message in first.into_iter().chain(rx.try_iter()) {
            match message {
                Message::SessionsChanged => sessions_changed = true,
                Message::PropertiesChanged(key) => {
                    properties_changed.insert(key);
                }
                Message::PlaybackChanged => {}
                Message::Command(key, command) => commands.push((key, command)),
            }
        }

        if sessions_changed {
            let mut artwork = HashMap::new();
            for old in watched.drain(..) {
                if let Some(cover) = old.unsubscribe() {
                    artwork.insert(cover.0, cover.1);
                }
            }
            for (key, session) in keyed_sessions(manager)? {
                let mut session = WatchedSession::subscribe(key, session, tx);
                session.artwork = artwork.remove(&session.key);
                properties_changed.insert(session.key.clone());
                watched.push(session);
            }
        }

        for (key, command) in commands {
            let Some(session) = watched.iter().find(|s| s.key == key) else {
                log::debug!("media command {command:?} for a session that is gone: {key}");
                continue;
            };
            if let Err(err) = session.execute(command) {
                log::warn!("media command {command:?} failed: {err}");
            }
        }

        // The thumbnail often arrives a moment after the title, so it is re-read on every
        // property change and keyed by its content rather than by the track name.
        for session in watched.iter_mut().filter(|s| properties_changed.contains(&s.key)) {
            match read_artwork(&session.session) {
                Ok(artwork) => {
                    if artwork.as_ref().map(|a| &a.key) != session.artwork.as_ref().map(|a| &a.key) {
                        session.artwork = artwork.map(Arc::new);
                    }
                }
                Err(err) => log::debug!("artwork unavailable for {}: {err}", session.key),
            }
        }

        let players: Vec<Player> = watched.iter().filter_map(WatchedSession::player).collect();
        if players != *last {
            *last = players.clone();
            on_update(players);
        }
    }
    Ok(())
}

/// Sessions with their player ids. The AppUserModelID is the id; an app with several sessions
/// gets a counter appended to the later ones. A session that cannot be read (one closing right
/// now) is left out; only a failed list is an error.
fn keyed_sessions(manager: &SessionManager) -> windows::core::Result<Vec<(String, Session)>> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut sessions = Vec::new();
    for session in manager.GetSessions()? {
        let app = match session.SourceAppUserModelId() {
            Ok(app) => app.to_string(),
            Err(err) => {
                log::debug!("skipping a media session that cannot be read: {err}");
                continue;
            }
        };
        let count = seen.entry(app.clone()).or_default();
        let key = if *count == 0 { app } else { format!("{app}#{count}") };
        *count += 1;
        sessions.push((key, session));
    }
    Ok(sessions)
}

fn session_keys(manager: &SessionManager) -> windows::core::Result<Vec<String>> {
    Ok(keyed_sessions(manager)?.into_iter().map(|(key, _)| key).collect())
}

fn keys(watched: &[WatchedSession]) -> Vec<String> {
    watched.iter().map(|s| s.key.clone()).collect()
}

struct WatchedSession {
    key: String,
    session: Session,
    artwork: Option<Arc<Artwork>>,
    tokens: [i64; 3],
}

impl WatchedSession {
    fn subscribe(key: String, session: Session, tx: &Sender<Message>) -> Self {
        let properties_tx = tx.clone();
        let properties_key = key.clone();
        let properties = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = properties_tx.send(Message::PropertiesChanged(properties_key.clone()));
            Ok(())
        }));
        let playback_tx = tx.clone();
        let playback = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            let _ = playback_tx.send(Message::PlaybackChanged);
            Ok(())
        }));
        let timeline_tx = tx.clone();
        let timeline = session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = timeline_tx.send(Message::PlaybackChanged);
            Ok(())
        }));
        Self {
            key,
            session,
            artwork: None,
            tokens: [properties.unwrap_or(0), playback.unwrap_or(0), timeline.unwrap_or(0)],
        }
    }

    /// Stops listening and hands back the artwork, so a resubscribed session keeps its cover.
    fn unsubscribe(self) -> Option<(String, Arc<Artwork>)> {
        let [properties, playback, timeline] = self.tokens;
        let _ = self.session.RemoveMediaPropertiesChanged(properties);
        let _ = self.session.RemovePlaybackInfoChanged(playback);
        let _ = self.session.RemoveTimelinePropertiesChanged(timeline);
        self.artwork.map(|artwork| (self.key, artwork))
    }

    fn execute(&self, command: MediaCommand) -> windows::core::Result<()> {
        let operation = match command {
            MediaCommand::TogglePlayPause => self.session.TryTogglePlayPauseAsync()?,
            MediaCommand::Next => self.session.TrySkipNextAsync()?,
            MediaCommand::Previous => self.session.TrySkipPreviousAsync()?,
        };
        operation.join()?;
        Ok(())
    }

    /// The session as a player, or `None` if it is stopped or cannot be read.
    fn player(&self) -> Option<Player> {
        match read_now_playing(&self.session) {
            Ok(Some(mut info)) => {
                info.artwork_key = self.artwork.as_ref().map(|a| a.key.clone());
                Some(Player { id: self.key.clone(), info, artwork: self.artwork.clone() })
            }
            Ok(None) => None,
            Err(err) => {
                log::debug!("could not read the media session {}: {err}", self.key);
                None
            }
        }
    }
}

fn read_now_playing(session: &Session) -> windows::core::Result<Option<NowPlaying>> {
    let playback = session.GetPlaybackInfo()?;
    let status = playback.PlaybackStatus()?;
    // Closed, Opened and Stopped sessions have nothing to show; apps leave them around for days.
    if !matches!(status, PlaybackStatus::Playing | PlaybackStatus::Paused | PlaybackStatus::Changing) {
        return Ok(None);
    }
    let properties = session.TryGetMediaPropertiesAsync()?.join()?;
    let title = properties.Title()?.to_string();
    if title.trim().is_empty() {
        return Ok(None);
    }
    let controls = playback.Controls()?;
    let timeline = session.GetTimelineProperties()?;

    let start = timeline.StartTime()?.Duration;
    let duration = timeline.EndTime()?.Duration - start;
    let position = timeline.Position()?.Duration - start;
    let sampled_at = timeline.LastUpdatedTime()?.UniversalTime;
    let has_timeline = duration > 0;

    Ok(Some(NowPlaying {
        source_app: session.SourceAppUserModelId()?.to_string(),
        title,
        artist: properties.Artist()?.to_string(),
        album: properties.AlbumTitle()?.to_string(),
        playing: status == PlaybackStatus::Playing,
        position_ms: has_timeline.then(|| ticks_to_ms(position)),
        duration_ms: has_timeline.then(|| ticks_to_ms(duration)),
        position_sampled_at: (has_timeline && sampled_at > UNIX_EPOCH_TICKS)
            .then(|| ticks_to_ms(sampled_at - UNIX_EPOCH_TICKS)),
        artwork_key: None,
        can_toggle: controls.IsPlayPauseToggleEnabled()?,
        can_next: controls.IsNextEnabled()?,
        can_previous: controls.IsPreviousEnabled()?,
    }))
}

fn read_artwork(session: &Session) -> windows::core::Result<Option<Artwork>> {
    let properties = session.TryGetMediaPropertiesAsync()?.join()?;
    let Ok(thumbnail) = properties.Thumbnail() else {
        return Ok(None);
    };
    let stream = thumbnail.OpenReadAsync()?.join()?;
    let size = u32::try_from(stream.Size()?).unwrap_or(u32::MAX);
    if size == 0 || size > MAX_ARTWORK_BYTES {
        return Ok(None);
    }
    let buffer = stream
        .ReadAsync(&Buffer::Create(size)?, size, InputStreamOptions::None)?
        .join()?;
    let mut bytes = vec![0u8; buffer.Length()? as usize];
    DataReader::FromBuffer(&buffer)?.ReadBytes(&mut bytes)?;

    // Apps report odd content types (Telegram sends "image/jpeg,image/jpe,image/jpg"), so the
    // bytes decide and the reported type is only a fallback.
    let mime = match sniff_image_type(&bytes) {
        Some(mime) => mime.to_string(),
        None => {
            let reported = stream.ContentType()?.to_string();
            let first = reported.split([',', ';']).next().unwrap_or_default().trim().to_string();
            if first.starts_with("image/") { first } else { return Ok(None) }
        }
    };
    Ok(Some(Artwork::from_bytes(&mime, &bytes)))
}

fn sniff_image_type(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [b'B', b'M', ..] => Some("image/bmp"),
        _ => None,
    }
}

fn ticks_to_ms(ticks: i64) -> u64 {
    (ticks.max(0) / 10_000) as u64
}
