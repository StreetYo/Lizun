//! Now playing via Global System Media Transport Controls (SMTC): every app that shows up in
//! the Windows media flyout (Spotify, browsers, Apple Music, ...) is visible here.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use base64::Engine;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

use crate::media::{Artwork, MediaCommand, MediaUpdate, NowPlaying};

/// Artwork larger than this is ignored; real covers are a few hundred kilobytes at most.
const MAX_ARTWORK_BYTES: u32 = 8 * 1024 * 1024;

/// Offset between the Windows epoch (1601) and the Unix epoch, in 100 ns ticks.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

enum Message {
    SessionChanged,
    PropertiesChanged,
    PlaybackChanged,
    Command(MediaCommand),
}

pub struct MediaController {
    tx: Sender<Message>,
}

impl MediaController {
    pub fn send(&self, command: MediaCommand) -> Result<(), String> {
        self.tx
            .send(Message::Command(command))
            .map_err(|_| "the media thread has stopped".to_string())
    }
}

pub fn spawn_media_watcher(on_update: impl Fn(MediaUpdate) + Send + 'static) -> Option<MediaController> {
    let (tx, rx) = mpsc::channel();
    let thread_tx = tx.clone();
    let spawned = thread::Builder::new().name("lizun-media".into()).spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        if let Err(err) = watch(thread_tx, rx, on_update) {
            log::warn!("media watcher stopped: {err}");
        }
    });
    match spawned {
        Ok(_) => Some(MediaController { tx }),
        Err(err) => {
            log::warn!("could not start the media thread: {err}");
            None
        }
    }
}

/// All WinRT calls happen on this thread. Event callbacks only post messages back to it, and
/// bursts of messages are coalesced so a track change produces one update.
fn watch(tx: Sender<Message>, rx: Receiver<Message>, on_update: impl Fn(MediaUpdate)) -> windows::core::Result<()> {
    let manager = SessionManager::RequestAsync()?.join()?;
    let session_tx = tx.clone();
    manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = session_tx.send(Message::SessionChanged);
        Ok(())
    }))?;

    let mut watched: Option<WatchedSession> = None;
    let mut last: Option<NowPlaying> = None;
    let mut artwork_key: Option<String> = None;
    let _ = tx.send(Message::SessionChanged);

    while let Ok(first) = rx.recv() {
        let mut session_changed = false;
        let mut properties_changed = false;
        let mut commands = Vec::new();
        for message in std::iter::once(first).chain(rx.try_iter()) {
            match message {
                Message::SessionChanged => session_changed = true,
                Message::PropertiesChanged => properties_changed = true,
                Message::PlaybackChanged => {}
                Message::Command(command) => commands.push(command),
            }
        }

        if session_changed {
            if let Some(old) = watched.take() {
                old.unsubscribe();
            }
            watched = manager.GetCurrentSession().ok().map(|session| WatchedSession::subscribe(session, &tx));
            artwork_key = None;
            properties_changed = true;
        }

        let Some(current) = &watched else {
            if last.take().is_some() {
                on_update(MediaUpdate::NowPlaying(None));
            }
            continue;
        };

        for command in commands {
            if let Err(err) = current.execute(command) {
                log::warn!("media command {command:?} failed: {err}");
            }
        }

        // The thumbnail often arrives a moment after the title, so it is re-read on every
        // property change and keyed by its content rather than by the track name.
        if properties_changed {
            match read_artwork(&current.session) {
                Ok(Some(artwork)) => {
                    if artwork_key.as_deref() != Some(artwork.key.as_str()) {
                        artwork_key = Some(artwork.key.clone());
                        on_update(MediaUpdate::Artwork(artwork));
                    }
                }
                Ok(None) => artwork_key = None,
                Err(err) => log::debug!("artwork unavailable: {err}"),
            }
        }

        let snapshot = match read_now_playing(&current.session) {
            Ok(mut snapshot) => {
                snapshot.artwork_key = artwork_key.clone();
                Some(snapshot)
            }
            Err(err) => {
                log::debug!("could not read the media session: {err}");
                None
            }
        };
        if snapshot != last {
            last = snapshot.clone();
            on_update(MediaUpdate::NowPlaying(snapshot));
        }
    }
    Ok(())
}

struct WatchedSession {
    session: Session,
    tokens: [i64; 3],
}

impl WatchedSession {
    fn subscribe(session: Session, tx: &Sender<Message>) -> Self {
        let properties_tx = tx.clone();
        let properties = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = properties_tx.send(Message::PropertiesChanged);
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
            session,
            tokens: [properties.unwrap_or(0), playback.unwrap_or(0), timeline.unwrap_or(0)],
        }
    }

    fn unsubscribe(self) {
        let [properties, playback, timeline] = self.tokens;
        let _ = self.session.RemoveMediaPropertiesChanged(properties);
        let _ = self.session.RemovePlaybackInfoChanged(playback);
        let _ = self.session.RemoveTimelinePropertiesChanged(timeline);
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
}

fn read_now_playing(session: &Session) -> windows::core::Result<NowPlaying> {
    let properties = session.TryGetMediaPropertiesAsync()?.join()?;
    let playback = session.GetPlaybackInfo()?;
    let controls = playback.Controls()?;
    let timeline = session.GetTimelineProperties()?;

    let start = timeline.StartTime()?.Duration;
    let duration = timeline.EndTime()?.Duration - start;
    let position = timeline.Position()?.Duration - start;
    let sampled_at = timeline.LastUpdatedTime()?.UniversalTime;
    let has_timeline = duration > 0;

    Ok(NowPlaying {
        source_app: session.SourceAppUserModelId()?.to_string(),
        title: properties.Title()?.to_string(),
        artist: properties.Artist()?.to_string(),
        album: properties.AlbumTitle()?.to_string(),
        playing: playback.PlaybackStatus()? == PlaybackStatus::Playing,
        position_ms: has_timeline.then(|| ticks_to_ms(position)),
        duration_ms: has_timeline.then(|| ticks_to_ms(duration)),
        position_sampled_at: (has_timeline && sampled_at > UNIX_EPOCH_TICKS)
            .then(|| ticks_to_ms(sampled_at - UNIX_EPOCH_TICKS)),
        artwork_key: None,
        can_toggle: controls.IsPlayPauseToggleEnabled()?,
        can_next: controls.IsNextEnabled()?,
        can_previous: controls.IsPreviousEnabled()?,
    })
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
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(Some(Artwork {
        key: format!("{:016x}", hasher.finish()),
        data_url: format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
    }))
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
