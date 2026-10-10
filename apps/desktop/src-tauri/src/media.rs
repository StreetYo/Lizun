//! Now-playing state shared by the island UI and widgets. Media comes from two sources: the
//! system media sessions (the platform backend) and browser tabs reported by the Lizun browser
//! extension (`browser`). Each source reports every player it sees, and one of them is picked for
//! the island; the UI reads it through events and commands.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT_CHANGED: &str = "media://changed";
/// The list of players for the source switcher changed.
pub const EVENT_PLAYERS: &str = "media://players";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// The app that owns the player: an AppUserModelID for system sessions, the site name
    /// (such as "YouTube") for browser tabs.
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

#[derive(Debug)]
pub struct Artwork {
    pub key: String,
    pub data_url: String,
}

impl Artwork {
    /// Artwork keyed by a hash of its bytes.
    pub fn from_bytes(mime: &str, bytes: &[u8]) -> Self {
        Self::from_data_url(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    }

    /// Artwork keyed by a hash of the image it encodes.
    pub fn from_data_url(data_url: String) -> Self {
        let mut hasher = DefaultHasher::new();
        data_url.hash(&mut hasher);
        Self { key: format!("{:016x}", hasher.finish()), data_url }
    }
}

/// One player a source can see, such as an app's media session or a browser tab. Sources leave
/// out stopped players, so every player here is playing or paused.
#[derive(Debug, Clone)]
pub struct Player {
    /// Unique within its source.
    pub id: String,
    pub info: NowPlaying,
    /// The image `info.artwork_key` refers to.
    pub artwork: Option<Arc<Artwork>>,
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        // The artwork is identified by `info.artwork_key`.
        self.id == other.id && self.info == other.info
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    System,
    Browser,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
}

struct Tracked {
    source: Source,
    player: Player,
    /// Clock value at which the player last started playing.
    started_at: u64,
    /// Clock value at which the player was last seen playing.
    played_at: u64,
}

impl Tracked {
    /// Identifies the player across sources; the UI selects players by it.
    fn key(&self) -> String {
        let source = match self.source {
            Source::System => "system",
            Source::Browser => "browser",
        };
        format!("{source}:{}", self.player.id)
    }
}

/// A player as the source switcher lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSummary {
    pub key: String,
    pub source_app: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub shown: bool,
}

/// Picks the player for the island: the one the user chose, until another player starts;
/// otherwise the one that started playing most recently, or, when nothing plays, the one that
/// played most recently.
#[derive(Default)]
struct Arbiter {
    players: Vec<Tracked>,
    /// Counts updates; orders the moments players start and stop.
    clock: u64,
    shown: Option<(Source, String)>,
    /// The player the user chose, and the clock value at that moment.
    chosen: Option<(String, u64)>,
}

impl Arbiter {
    /// Replaces every player of `source`.
    fn update(&mut self, source: Source, players: Vec<Player>) {
        self.clock += 1;
        let mut previous: HashMap<String, Tracked> = HashMap::new();
        let mut kept = Vec::with_capacity(self.players.len());
        for tracked in self.players.drain(..) {
            if tracked.source == source {
                previous.insert(tracked.player.id.clone(), tracked);
            } else {
                kept.push(tracked);
            }
        }
        self.players = kept;

        for player in players {
            let old = previous.remove(&player.id);
            let was_playing = old.as_ref().is_some_and(|old| old.player.info.playing);
            let mut started_at = old.as_ref().map_or(0, |old| old.started_at);
            let mut played_at = old.as_ref().map_or(0, |old| old.played_at);
            if player.info.playing {
                if !was_playing {
                    started_at = self.clock;
                }
                played_at = self.clock;
            }
            self.players.push(Tracked { source, player, started_at, played_at });
        }

        self.refresh()
    }

    /// Shows the player with `key` until another player starts playing.
    fn choose(&mut self, key: &str) {
        if self.candidates().any(|t| t.key() == key) {
            self.chosen = Some((key.to_string(), self.clock));
        }
        self.refresh()
    }

    fn refresh(&mut self) {
        let index = self.pick();
        self.shown = index.map(|i| (self.players[i].source, self.players[i].player.id.clone()));
    }

    fn pick(&mut self) -> Option<usize> {
        if let Some((key, at)) = self.chosen.clone() {
            let started_since = self.candidates().any(|t| t.player.info.playing && t.started_at > at);
            match self.players.iter().position(|t| t.key() == key) {
                Some(index) if !started_since => return Some(index),
                _ => self.chosen = None,
            }
        }

        let is_shown = |t: &Tracked| self.shown.as_ref().is_some_and(|(s, id)| *s == t.source && *id == t.player.id);
        let rank = |t: &Tracked| {
            let at = if t.player.info.playing { t.started_at } else { t.played_at };
            // The shown player wins ties, so equal candidates do not take turns.
            (t.player.info.playing, at, is_shown(t))
        };
        let best = self.candidates().max_by_key(|t| rank(t))?.key();
        self.players.iter().position(|t| t.key() == best)
    }

    /// Players that can be shown. A tab of a browser that publishes its media to the system is
    /// seen twice, as the tab and as the browser's system session; the tab has the page's own
    /// cover and controls, so the session is left out.
    fn candidates(&self) -> impl Iterator<Item = &Tracked> {
        let tabs: HashSet<String> = self
            .players
            .iter()
            .filter(|tab| tab.source == Source::Browser)
            .map(|tab| normalize(&tab.player.info.title))
            .collect();
        self.players
            .iter()
            .filter(move |t| t.source == Source::Browser || !tabs.contains(&normalize(&t.player.info.title)))
    }

    /// Every player that can be shown, in a stable order.
    fn summaries(&self) -> Vec<PlayerSummary> {
        let shown = self.shown().map(Tracked::key);
        let mut summaries: Vec<PlayerSummary> = self
            .candidates()
            .map(|t| PlayerSummary {
                key: t.key(),
                source_app: t.player.info.source_app.clone(),
                title: t.player.info.title.clone(),
                artist: t.player.info.artist.clone(),
                playing: t.player.info.playing,
                shown: shown.as_deref() == Some(t.key().as_str()),
            })
            .collect();
        summaries.sort_by(|a, b| a.key.cmp(&b.key));
        summaries
    }

    fn shown(&self) -> Option<&Tracked> {
        let (source, id) = self.shown.as_ref()?;
        self.players.iter().find(|t| t.source == *source && t.player.id == *id)
    }

    fn artwork(&self, key: &str) -> Option<&Artwork> {
        self.players
            .iter()
            .filter_map(|t| t.player.artwork.as_deref())
            .find(|artwork| artwork.key == key)
    }
}

fn normalize(title: &str) -> String {
    title.trim().to_lowercase()
}

#[derive(Default)]
pub struct MediaState {
    arbiter: Mutex<Arbiter>,
    now_playing: Mutex<Option<NowPlaying>>,
    players: Mutex<Vec<PlayerSummary>>,
    controller: Mutex<Option<crate::platform::MediaController>>,
}

pub fn start(app: &AppHandle) {
    app.manage(MediaState::default());
    let handle = app.clone();
    let controller =
        crate::platform::spawn_media_watcher(move |players| update(&handle, Source::System, players));
    *app.state::<MediaState>().controller.lock().unwrap() = controller;
}

/// Replaces the players of one source and tells the UI what changed.
pub fn update(app: &AppHandle, source: Source, players: Vec<Player>) {
    publish(app, |arbiter| {
        arbiter.update(source, players);
    });
}

/// Changes the arbiter and emits the shown player and the player list if they changed.
fn publish(app: &AppHandle, change: impl FnOnce(&mut Arbiter)) {
    let state = app.state::<MediaState>();
    let (now_playing, players) = {
        let mut arbiter = state.arbiter.lock().unwrap();
        change(&mut arbiter);
        (arbiter.shown().map(|t| t.player.info.clone()), arbiter.summaries())
    };
    if replace(&state.now_playing, now_playing.clone())
        && let Err(err) = app.emit(EVENT_CHANGED, now_playing)
    {
        log::warn!("failed to emit media update: {err}");
    }
    if replace(&state.players, players.clone())
        && let Err(err) = app.emit(EVENT_PLAYERS, players)
    {
        log::warn!("failed to emit the player list: {err}");
    }
}

/// Stores `value` and returns whether it differs from the stored one.
fn replace<T: PartialEq>(slot: &Mutex<T>, value: T) -> bool {
    let mut slot = slot.lock().unwrap();
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

#[tauri::command]
pub fn media_snapshot(state: State<'_, MediaState>) -> Option<NowPlaying> {
    state.now_playing.lock().unwrap().clone()
}

#[tauri::command]
pub fn media_players(state: State<'_, MediaState>) -> Vec<PlayerSummary> {
    state.players.lock().unwrap().clone()
}

/// Shows another player; the choice holds until some other player starts playing.
#[tauri::command]
pub fn media_select(app: AppHandle, key: String) {
    publish(&app, |arbiter| {
        arbiter.choose(&key);
    });
}

#[tauri::command]
pub fn media_artwork(state: State<'_, MediaState>, key: String) -> Option<String> {
    state.arbiter.lock().unwrap().artwork(&key).map(|a| a.data_url.clone())
}

#[tauri::command]
pub fn media_control(app: AppHandle, command: MediaCommand) -> Result<(), String> {
    control(&app, command)
}

/// Sends a command to the player on the island.
pub fn control(app: &AppHandle, command: MediaCommand) -> Result<(), String> {
    let state = app.state::<MediaState>();
    let (source, id, playing) = {
        let arbiter = state.arbiter.lock().unwrap();
        let shown = arbiter.shown().ok_or("nothing is playing")?;
        (shown.source, shown.player.id.clone(), shown.player.info.playing)
    };
    match source {
        Source::System => {
            let controller = state.controller.lock().unwrap();
            let controller = controller.as_ref().ok_or("media backend is unavailable")?;
            controller.send(&id, command)
        }
        Source::Browser => crate::browser::control(app, &id, command, playing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(id: &str, title: &str, playing: bool) -> Player {
        Player {
            id: id.into(),
            info: NowPlaying { title: title.into(), playing, ..NowPlaying::default() },
            artwork: None,
        }
    }

    fn shown(arbiter: &mut Arbiter, source: Source, players: Vec<Player>) -> Option<String> {
        arbiter.update(source, players);
        arbiter.shown().map(|t| t.player.id.clone())
    }

    #[test]
    fn prefers_the_player_that_started_last() {
        let mut arbiter = Arbiter::default();
        assert_eq!(shown(&mut arbiter, Source::System, vec![player("spotify", "Song", true)]).as_deref(), Some("spotify"));
        assert_eq!(shown(&mut arbiter, Source::Browser, vec![player("tab", "Video", true)]).as_deref(), Some("tab"));
        // Spotify keeps playing; a position update must not steal the island back.
        let mut moved = player("spotify", "Song", true);
        moved.info.position_ms = Some(5000);
        assert_eq!(shown(&mut arbiter, Source::System, vec![moved]).as_deref(), Some("tab"));
    }

    #[test]
    fn falls_back_to_the_last_paused_player() {
        let mut arbiter = Arbiter::default();
        shown(&mut arbiter, Source::System, vec![player("a", "A", true), player("b", "B", false)]);
        assert_eq!(
            shown(&mut arbiter, Source::System, vec![player("a", "A", false), player("b", "B", false)]).as_deref(),
            Some("a")
        );
        assert_eq!(shown(&mut arbiter, Source::System, vec![]), None);
    }

    #[test]
    fn keeps_the_shown_player_on_ties() {
        let mut arbiter = Arbiter::default();
        let first = shown(&mut arbiter, Source::System, vec![player("a", "A", false), player("b", "B", false)]);
        let again = shown(&mut arbiter, Source::System, vec![player("b", "B", false), player("a", "A", false)]);
        assert_eq!(first, again);
    }

    #[test]
    fn keeps_the_chosen_player_until_another_starts() {
        let mut arbiter = Arbiter::default();
        shown(&mut arbiter, Source::Browser, vec![player("music", "Song", true), player("video", "Clip", false)]);
        arbiter.choose("browser:video");
        assert_eq!(arbiter.shown().map(|t| t.player.id.as_str()), Some("video"));

        // The music keeps playing and moves on to the next track: the choice holds.
        assert_eq!(
            shown(&mut arbiter, Source::Browser, vec![player("music", "Next song", true), player("video", "Clip", false)])
                .as_deref(),
            Some("video")
        );
        // Something starts playing: it takes over.
        shown(&mut arbiter, Source::Browser, vec![player("music", "Next song", false), player("video", "Clip", false)]);
        assert_eq!(
            shown(&mut arbiter, Source::Browser, vec![player("music", "Next song", true), player("video", "Clip", false)])
                .as_deref(),
            Some("music")
        );
    }

    #[test]
    fn lists_players_without_duplicates() {
        let mut arbiter = Arbiter::default();
        shown(&mut arbiter, Source::Browser, vec![player("tab", "Video", true)]);
        shown(&mut arbiter, Source::System, vec![player("chrome", "video", true), player("spotify", "Song", false)]);
        let summaries = arbiter.summaries();
        let keys: Vec<&str> = summaries.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["browser:tab", "system:spotify"]);
        assert!(summaries[0].shown && !summaries[1].shown);
        // Choosing an unknown player changes nothing.
        arbiter.choose("system:chrome");
        assert_eq!(arbiter.shown().map(|t| t.key()).as_deref(), Some("browser:tab"));
    }

    #[test]
    fn prefers_the_tab_over_its_browser_session() {
        let mut arbiter = Arbiter::default();
        shown(&mut arbiter, Source::Browser, vec![player("tab", "Video ", true)]);
        assert_eq!(
            shown(&mut arbiter, Source::System, vec![player("chrome", "video", true)]).as_deref(),
            Some("tab")
        );
    }
}
