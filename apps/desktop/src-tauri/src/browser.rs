//! Media playing in browser tabs, reported by the Lizun browser extension
//! (a separate repository, lizun-browser-extension).
//!
//! Some browsers, Yandex Browser among them, do not publish their media to the system, so the
//! system media backend cannot see them. The extension watches every tab's media and connects
//! to a WebSocket server here on 127.0.0.1; the port is a preference that the extension's popup
//! must match. The messages are JSON, described in docs/browser-protocol.md.

use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

use crate::i18n::tf;
use crate::media::{Artwork, MediaCommand, NowPlaying, Player, Source};

pub const EVENT_STATUS: &str = "browser-media://status";
pub const DEFAULT_PORT: u16 = 6869;
pub const PROTOCOL: u32 = 1;

/// How often binding is retried while the port is taken.
const BIND_RETRY: Duration = Duration::from_secs(5);
const ACCEPT_POLL: Duration = Duration::from_millis(250);
/// Connections wake up this often to send queued commands.
const READ_POLL: Duration = Duration::from_millis(50);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

// Anything local can connect, so what one connection may hold is bounded.
/// Most connections open at once; each browser opens one.
const MAX_CONNECTIONS: usize = 8;
/// Most frames with media one connection reports.
const MAX_TABS: usize = 100;
/// Longest cover, as a data URL. Covers are usually tens of kilobytes.
const MAX_ARTWORK_BYTES: usize = 2 * 1024 * 1024;
/// Most bytes of covers one connection keeps for all its tabs.
const MAX_ARTWORK_TOTAL: usize = 16 * 1024 * 1024;
/// Largest message read: the largest cover and the JSON around it.
const MAX_MESSAGE_BYTES: usize = MAX_ARTWORK_BYTES + 64 * 1024;
/// Longest tab id, cover URL or text kept. Longer text is cut; longer ids and URLs are ignored.
const MAX_TEXT_BYTES: usize = 1024;
/// The actions the island can ask for; others are dropped.
const ACTIONS: [&str; 5] = ["play", "pause", "next", "previous", "seek"];

/// Origins the extension connects from. Web pages can open WebSockets to localhost too, so
/// they are turned away; non-browser clients send no origin and are local programs anyway.
const EXTENSION_ORIGINS: [&str; 3] = ["chrome-extension://", "moz-extension://", "safari-web-extension://"];

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub port: u16,
    pub listening: bool,
    /// Why the server is not listening, such as the port being used by another app.
    pub error: Option<String>,
    /// Browsers connected right now.
    pub browsers: Vec<String>,
}

/// A message from the extension.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Incoming {
    Hello { protocol: u32, version: String },
    Player { id: String, player: TabState },
    Artwork { id: String, url: String, data: String },
    Removed { id: String },
    /// Keeps the extension's background script alive; nothing to do.
    Ping,
    /// Messages added by newer extensions.
    #[serde(other)]
    Unknown,
}

/// Media in one frame of one tab.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabState {
    site: String,
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
    artwork_url: Option<String>,
    playing: bool,
    position: Option<f64>,
    duration: Option<f64>,
    /// Unix time in milliseconds at which `position` was read.
    sampled_at: u64,
    #[serde(default)]
    actions: Vec<String>,
}

/// A message to the extension.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Outgoing<'a> {
    Hello { protocol: u32, app: &'a str, version: &'a str },
    Command {
        id: &'a str,
        action: &'a str,
        /// Seconds, for `seek` only.
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Config {
    enabled: bool,
    port: u16,
    /// Bumped on every change; listeners and connections of older generations shut down.
    generation: u64,
}

#[derive(Default)]
struct Listener {
    listening: bool,
    error: Option<String>,
}

struct Connection {
    browser: String,
    commands: mpsc::Sender<String>,
    players: Vec<Player>,
}

pub struct BrowserMedia {
    app: AppHandle,
    config: Mutex<Config>,
    config_changed: Condvar,
    listener: Mutex<Listener>,
    connections: Mutex<BTreeMap<u64, Connection>>,
    /// Held while players are collected and handed on, so updates arrive in order.
    publishing: Mutex<()>,
    last_status: Mutex<Status>,
    next_connection: AtomicU64,
    /// Connections being served, counting those still in the handshake.
    open: Arc<AtomicUsize>,
}

/// Holds a place among the MAX_CONNECTIONS open connections until dropped.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(open: &Arc<AtomicUsize>) -> Option<Self> {
        let taken = open.fetch_add(1, Ordering::SeqCst);
        let slot = Slot(open.clone());
        (taken < MAX_CONNECTIONS).then_some(slot)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn start(app: &AppHandle, enabled: bool, port: u16) {
    let server = Arc::new(BrowserMedia {
        app: app.clone(),
        config: Mutex::new(Config { enabled, port, generation: 0 }),
        config_changed: Condvar::new(),
        listener: Mutex::new(Listener::default()),
        connections: Mutex::new(BTreeMap::new()),
        publishing: Mutex::new(()),
        last_status: Mutex::new(Status::default()),
        next_connection: AtomicU64::new(1),
        open: Arc::new(AtomicUsize::new(0)),
    });
    app.manage(server.clone());
    if let Err(err) = thread::Builder::new().name("lizun-browser-media".into()).spawn(move || server.listen()) {
        log::warn!("could not start the browser media thread: {err}");
    }
}

/// Applies changed preferences; a new port takes effect immediately.
pub fn configure(app: &AppHandle, enabled: bool, port: u16) {
    if let Some(server) = app.try_state::<Arc<BrowserMedia>>() {
        server.configure(enabled, port);
    }
}

/// Sends `command` to the tab that `media::Player::id` names.
pub fn control(app: &AppHandle, player_id: &str, command: MediaCommand, playing: bool) -> Result<(), String> {
    let server = app.try_state::<Arc<BrowserMedia>>().ok_or("browser media is unavailable")?;
    let (connection, tab) = parse_player_id(player_id).ok_or("unknown browser player")?;
    let action = match command {
        MediaCommand::TogglePlayPause if playing => "pause",
        MediaCommand::TogglePlayPause => "play",
        MediaCommand::Next => "next",
        MediaCommand::Previous => "previous",
        MediaCommand::Seek(_) => "seek",
    };
    let position = match command {
        MediaCommand::Seek(ms) => Some(ms as f64 / 1000.0),
        _ => None,
    };
    let message = serde_json::to_string(&Outgoing::Command { id: tab, action, position }).map_err(|e| e.to_string())?;
    let connections = server.connections.lock().unwrap();
    let connection = connections.get(&connection).ok_or("the browser has disconnected")?;
    connection.commands.send(message).map_err(|_| "the browser has disconnected".to_string())
}

#[tauri::command]
pub fn browser_media_status(server: State<'_, Arc<BrowserMedia>>) -> Status {
    server.status()
}

impl BrowserMedia {
    fn configure(&self, enabled: bool, port: u16) {
        {
            let mut config = self.config.lock().unwrap();
            if config.enabled == enabled && config.port == port {
                return;
            }
            *config = Config { enabled, port, generation: config.generation + 1 };
        }
        self.config_changed.notify_all();
        self.publish_status();
    }

    fn generation(&self) -> u64 {
        self.config.lock().unwrap().generation
    }

    /// Sleeps until the configuration changes or `timeout` passes.
    fn wait_for_change(&self, generation: u64, timeout: Option<Duration>) {
        let config = self.config.lock().unwrap();
        let unchanged = |config: &mut Config| config.generation == generation;
        match timeout {
            Some(timeout) => drop(self.config_changed.wait_timeout_while(config, timeout, unchanged)),
            None => drop(self.config_changed.wait_while(config, unchanged)),
        }
    }

    /// Runs for the life of the app. A port that cannot be bound is retried, so another app
    /// holding it only disables browser media until the port is free or changed.
    fn listen(self: Arc<Self>) {
        loop {
            let config = *self.config.lock().unwrap();
            if !config.enabled {
                self.set_listener(false, None);
                self.wait_for_change(config.generation, None);
                continue;
            }

            let bound = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port)).and_then(|listener| {
                listener.set_nonblocking(true)?;
                Ok(listener)
            });
            let listener = match bound {
                Ok(listener) => listener,
                Err(err) => {
                    log::warn!("browser media cannot listen on port {}: {err}", config.port);
                    self.set_listener(false, Some(describe_bind_error(&err, config.port)));
                    self.wait_for_change(config.generation, Some(BIND_RETRY));
                    continue;
                }
            };
            log::info!("browser media listening on 127.0.0.1:{}", config.port);
            self.set_listener(true, None);

            while self.generation() == config.generation {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let Some(slot) = Slot::take(&self.open) else {
                            log::debug!("browser media turned a connection away: {MAX_CONNECTIONS} are open");
                            continue;
                        };
                        let server = self.clone();
                        let spawned = thread::Builder::new().name("lizun-browser-connection".into()).spawn(move || {
                            let _slot = slot;
                            server.serve(stream, config.generation);
                        });
                        if let Err(err) = spawned {
                            log::warn!("could not start a browser media connection: {err}");
                        }
                    }
                    Err(err) => {
                        if err.kind() != ErrorKind::WouldBlock {
                            log::debug!("browser media accept failed: {err}");
                        }
                        self.wait_for_change(config.generation, Some(ACCEPT_POLL));
                    }
                }
            }
        }
    }

    // The handshake callback has to return tungstenite's large error type.
    #[allow(clippy::result_large_err)]
    fn serve(self: Arc<Self>, stream: TcpStream, generation: u64) {
        let mut browser = "Browser";
        let socket = stream
            .set_nonblocking(false)
            .and_then(|()| stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)))
            .map_err(|err| err.to_string())
            .and_then(|()| {
                let config = WebSocketConfig::default()
                    .max_message_size(Some(MAX_MESSAGE_BYTES))
                    .max_frame_size(Some(MAX_MESSAGE_BYTES));
                let callback = |request: &Request, response: Response| {
                    check_origin(request)?;
                    let user_agent = request.headers().get("user-agent").and_then(|v| v.to_str().ok());
                    browser = browser_name(user_agent.unwrap_or_default());
                    Ok(response)
                };
                tungstenite::accept_hdr_with_config(stream, callback, Some(config)).map_err(|err| err.to_string())
            });
        let mut socket = match socket {
            Ok(socket) => socket,
            Err(err) => {
                log::debug!("browser media handshake failed: {err}");
                return;
            }
        };

        let id = self.next_connection.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.connections
            .lock()
            .unwrap()
            .insert(id, Connection { browser: browser.to_string(), commands: tx, players: Vec::new() });
        log::info!("{browser} connected to browser media");
        self.publish_status();

        if let Err(err) = self.run(id, generation, &mut socket, &rx) {
            log::debug!("{browser} browser media connection ended: {err}");
        }
        let _ = socket.close(None);
        let _ = socket.flush();

        self.connections.lock().unwrap().remove(&id);
        log::info!("{browser} disconnected from browser media");
        self.publish_players();
        self.publish_status();
    }

    fn run(
        &self,
        id: u64,
        generation: u64,
        socket: &mut WebSocket<TcpStream>,
        commands: &Receiver<String>,
    ) -> tungstenite::Result<()> {
        socket.get_ref().set_read_timeout(Some(READ_POLL))?;
        let hello = Outgoing::Hello { protocol: PROTOCOL, app: "Lizun", version: env!("CARGO_PKG_VERSION") };
        socket.send(Message::text(serde_json::to_string(&hello).unwrap_or_default()))?;
        let mut tabs = Tabs::default();
        loop {
            if self.generation() != generation {
                return Ok(());
            }
            for command in commands.try_iter() {
                socket.send(Message::text(command))?;
            }
            let message = match socket.read() {
                Ok(message) => message,
                Err(tungstenite::Error::Io(err)) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    continue;
                }
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return Ok(()),
                Err(err) => return Err(err),
            };
            let changed = match message {
                Message::Text(text) => tabs.handle(text.as_str()),
                Message::Close(_) => return Ok(()),
                _ => false,
            };
            if changed {
                let players = tabs.players(id);
                let mut connections = self.connections.lock().unwrap();
                let Some(connection) = connections.get_mut(&id) else { continue };
                if connection.players != players {
                    connection.players = players;
                    drop(connections);
                    self.publish_players();
                }
            }
        }
    }

    fn publish_players(&self) {
        let _order = self.publishing.lock().unwrap();
        let players: Vec<Player> =
            self.connections.lock().unwrap().values().flat_map(|c| c.players.iter().cloned()).collect();
        crate::media::update(&self.app, Source::Browser, players);
    }

    fn set_listener(&self, listening: bool, error: Option<String>) {
        *self.listener.lock().unwrap() = Listener { listening, error };
        self.publish_status();
    }

    fn status(&self) -> Status {
        let config = *self.config.lock().unwrap();
        let listener = self.listener.lock().unwrap();
        Status {
            enabled: config.enabled,
            port: config.port,
            listening: config.enabled && listener.listening,
            error: listener.error.clone().filter(|_| config.enabled),
            browsers: self.connections.lock().unwrap().values().map(|c| c.browser.clone()).collect(),
        }
    }

    fn publish_status(&self) {
        let status = self.status();
        {
            let mut last = self.last_status.lock().unwrap();
            if *last == status {
                return;
            }
            *last = status.clone();
        }
        if let Err(err) = self.app.emit(EVENT_STATUS, status) {
            log::warn!("failed to emit the browser media status: {err}");
        }
    }
}

/// The tabs one browser reports.
#[derive(Default)]
struct Tabs {
    tabs: HashMap<String, Tab>,
}

struct Tab {
    state: TabState,
    /// The cover of `state.artwork_url`, once the extension has sent it.
    artwork: Option<Arc<Artwork>>,
}

impl Tabs {
    /// Applies a message; returns whether any tab changed.
    fn handle(&mut self, text: &str) -> bool {
        let message = match serde_json::from_str::<Incoming>(text) {
            Ok(message) => message,
            Err(err) => {
                log::debug!("ignoring a browser media message: {err}");
                return false;
            }
        };
        match message {
            Incoming::Hello { protocol, version } => {
                if protocol != PROTOCOL {
                    let version: String = version.chars().take(32).collect();
                    log::warn!("browser extension {version} speaks protocol {protocol}, Lizun {PROTOCOL}");
                }
                false
            }
            Incoming::Player { id, mut player } => {
                if id.len() > MAX_TEXT_BYTES {
                    return false;
                }
                player.clip();
                let full = self.tabs.len() >= MAX_TABS;
                match self.tabs.get_mut(&id) {
                    Some(tab) => {
                        if tab.state.artwork_url != player.artwork_url {
                            tab.artwork = None;
                        }
                        tab.state = player;
                        true
                    }
                    None if full => {
                        log::debug!("ignoring a browser tab: {MAX_TABS} tabs are playing already");
                        false
                    }
                    None => {
                        self.tabs.insert(id, Tab { state: player, artwork: None });
                        true
                    }
                }
            }
            Incoming::Artwork { id, url, data } => {
                if !data.starts_with("data:image/") || data.len() > MAX_ARTWORK_BYTES {
                    return false;
                }
                let others: usize = self
                    .tabs
                    .iter()
                    .filter(|(other, _)| **other != id)
                    .filter_map(|(_, tab)| tab.artwork.as_ref())
                    .map(|artwork| artwork.data_url.len())
                    .sum();
                if others + data.len() > MAX_ARTWORK_TOTAL {
                    log::debug!("ignoring a browser tab's cover: the covers of the other tabs take enough memory");
                    return false;
                }
                // Covers arrive after the track and may be late for a track that has moved on.
                let Some(tab) = self.tabs.get_mut(&id).filter(|tab| tab.state.artwork_url.as_deref() == Some(&url)) else {
                    return false;
                };
                tab.artwork = Some(Arc::new(Artwork::from_data_url(data)));
                true
            }
            Incoming::Removed { id } => self.tabs.remove(&id).is_some(),
            Incoming::Ping | Incoming::Unknown => false,
        }
    }

    fn players(&self, connection: u64) -> Vec<Player> {
        let mut players: Vec<Player> = self
            .tabs
            .iter()
            .filter(|(_, tab)| !tab.state.title.trim().is_empty())
            .map(|(id, tab)| Player {
                id: format!("{connection}:{id}"),
                info: tab.now_playing(),
                artwork: tab.artwork.clone(),
            })
            .collect();
        // A stable order, so an unchanged set of tabs compares equal.
        players.sort_by(|a, b| a.id.cmp(&b.id));
        players
    }
}

impl TabState {
    /// Cuts what nobody reads in full, so a tab holds little whatever the extension sends.
    fn clip(&mut self) {
        for text in [&mut self.site, &mut self.title, &mut self.artist, &mut self.album] {
            if text.len() > MAX_TEXT_BYTES {
                let mut end = MAX_TEXT_BYTES;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
            }
        }
        if self.artwork_url.as_ref().is_some_and(|url| url.len() > MAX_TEXT_BYTES) {
            self.artwork_url = None;
        }
        self.actions =
            ACTIONS.iter().filter(|action| self.actions.iter().any(|a| a == *action)).map(|a| a.to_string()).collect();
    }
}

impl Tab {
    fn now_playing(&self) -> NowPlaying {
        let state = &self.state;
        let can = |action: &str| state.actions.iter().any(|a| a == action);
        // Live streams report no duration, and their position means nothing.
        let duration_ms = state.duration.filter(|d| d.is_finite() && *d > 0.0).map(seconds_to_ms);
        let position_ms = duration_ms.and(state.position.filter(|p| p.is_finite())).map(seconds_to_ms);
        NowPlaying {
            source_app: state.site.clone(),
            title: state.title.trim().to_string(),
            artist: state.artist.clone(),
            album: state.album.clone(),
            playing: state.playing,
            position_ms,
            duration_ms,
            position_sampled_at: position_ms.map(|_| state.sampled_at),
            artwork_key: self.artwork.as_ref().map(|a| a.key.clone()),
            can_toggle: can(if state.playing { "pause" } else { "play" }),
            can_next: can("next"),
            can_previous: can("previous"),
            can_seek: duration_ms.is_some() && can("seek"),
        }
    }
}

fn seconds_to_ms(seconds: f64) -> u64 {
    (seconds.max(0.0) * 1000.0) as u64
}

/// Player ids are `<connection>:<tab id>`; the tab id contains colons of its own.
fn parse_player_id(id: &str) -> Option<(u64, &str)> {
    let (connection, tab) = id.split_once(':')?;
    Some((connection.parse().ok()?, tab))
}

#[allow(clippy::result_large_err)]
fn check_origin(request: &Request) -> Result<(), ErrorResponse> {
    let Some(origin) = request.headers().get("origin") else {
        return Ok(());
    };
    let origin = origin.to_str().unwrap_or_default();
    if EXTENSION_ORIGINS.iter().any(|scheme| origin.starts_with(scheme)) {
        return Ok(());
    }
    log::warn!("browser media refused a connection from {origin}");
    let mut response = ErrorResponse::new(Some("only the Lizun browser extension may connect".into()));
    *response.status_mut() = StatusCode::FORBIDDEN;
    Err(response)
}

fn browser_name(user_agent: &str) -> &'static str {
    const BROWSERS: [(&str, &str); 7] = [
        ("YaBrowser/", "Yandex Browser"),
        ("Edg/", "Microsoft Edge"),
        ("OPR/", "Opera"),
        ("Vivaldi/", "Vivaldi"),
        ("Firefox/", "Firefox"),
        ("Chrome/", "Chrome"),
        ("Safari/", "Safari"),
    ];
    BROWSERS.iter().find(|(token, _)| user_agent.contains(token)).map_or("Browser", |(_, name)| name)
}

fn describe_bind_error(err: &std::io::Error, port: u16) -> String {
    match err.kind() {
        ErrorKind::AddrInUse => tf("browser.portInUse", &[("port", &port)]),
        // Windows reserves port ranges for Hyper-V and WSL and refuses to bind them.
        ErrorKind::PermissionDenied => tf("browser.portReserved", &[("port", &port)]),
        _ => tf("browser.portUnavailable", &[("port", &port), ("error", err)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorded from the extension on music.yandex.ru.
    const PLAYER: &str = r#"{"type":"player","id":"601786579:0","player":{"title":"Bitch Please (Feat. Xzibit )","artist":"Snoop Dogg, Xzibit","album":"The Best Of Snoop Dogg","artworkUrl":"https://avatars.yandex.net/get-music-content/17722696/e8dc694a.a.26037-2/400x400","playing":true,"position":164,"duration":234.73333333333332,"playbackRate":1,"sampledAt":1791390467370,"actions":["play","pause","previous","next","seek"],"site":"music.yandex.ru"}}"#;
    const ARTWORK: &str = r#"{"type":"artwork","id":"601786579:0","url":"https://avatars.yandex.net/get-music-content/17722696/e8dc694a.a.26037-2/400x400","data":"data:image/jpeg;base64,/9j/4AAQ"}"#;

    #[test]
    fn reads_a_player_and_its_cover() {
        let mut tabs = Tabs::default();
        assert!(!tabs.handle(r#"{"type":"hello","protocol":1,"version":"0.1.0"}"#));
        assert!(tabs.handle(PLAYER));
        assert!(tabs.handle(ARTWORK));

        let players = tabs.players(3);
        assert_eq!(players.len(), 1);
        let player = &players[0];
        assert_eq!(player.id, "3:601786579:0");
        assert_eq!(parse_player_id(&player.id), Some((3, "601786579:0")));
        let info = &player.info;
        assert_eq!(info.source_app, "music.yandex.ru");
        assert_eq!(info.title, "Bitch Please (Feat. Xzibit )");
        assert_eq!(info.artist, "Snoop Dogg, Xzibit");
        assert!(info.playing);
        assert_eq!((info.position_ms, info.duration_ms), (Some(164_000), Some(234_733)));
        assert_eq!(info.position_sampled_at, Some(1791390467370));
        assert!(info.can_toggle && info.can_next && info.can_previous);
        assert_eq!(info.artwork_key, player.artwork.as_ref().map(|a| a.key.clone()));
        assert!(info.artwork_key.is_some());
    }

    #[test]
    fn drops_covers_of_previous_tracks() {
        let mut tabs = Tabs::default();
        tabs.handle(PLAYER);
        tabs.handle(ARTWORK);
        // The next track has another cover; the old one goes until the new one arrives.
        tabs.handle(&PLAYER.replace("400x400", "200x200"));
        assert!(tabs.players(1)[0].artwork.is_none());
        // A late cover of the previous track is ignored.
        assert!(!tabs.handle(ARTWORK));
    }

    #[test]
    fn removes_tabs_and_ignores_unknown_messages() {
        let mut tabs = Tabs::default();
        tabs.handle(PLAYER);
        assert!(!tabs.handle(r#"{"type":"ping"}"#));
        assert!(!tabs.handle(r#"{"type":"somethingNew","x":1}"#));
        assert!(!tabs.handle("not json"));
        assert!(tabs.handle(r#"{"type":"removed","id":"601786579:0"}"#));
        assert!(tabs.players(1).is_empty());
    }

    #[test]
    fn live_streams_have_no_timeline() {
        let mut tabs = Tabs::default();
        tabs.handle(r#"{"type":"player","id":"1:0","player":{"title":"Stream","artist":"","album":"","artworkUrl":null,"playing":true,"position":null,"duration":null,"playbackRate":1,"sampledAt":5,"actions":["play","pause"],"site":"kick.com"}}"#);
        let info = &tabs.players(1)[0].info;
        assert_eq!((info.position_ms, info.duration_ms, info.position_sampled_at), (None, None, None));
        assert!(info.can_toggle && !info.can_next);
    }

    #[test]
    fn writes_commands() {
        let command = Outgoing::Command { id: "601786579:0", action: "next", position: None };
        assert_eq!(
            serde_json::to_string(&command).unwrap(),
            r#"{"type":"command","id":"601786579:0","action":"next"}"#
        );
    }

    #[test]
    fn recognizes_browsers() {
        let yandex = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 YaBrowser/26.8.0.0 Safari/537.36";
        assert_eq!(browser_name(yandex), "Yandex Browser");
        assert_eq!(browser_name("Mozilla/5.0 Gecko/20100101 Firefox/141.0"), "Firefox");
        assert_eq!(browser_name(""), "Browser");
    }

    fn player_message(id: &str, title: &str, artwork_url: &str) -> String {
        serde_json::json!({
            "type": "player",
            "id": id,
            "player": {
                "site": "example.com", "title": title, "artworkUrl": artwork_url, "playing": true,
                "position": null, "duration": null, "sampledAt": 1, "actions": ["play", "pause", "play", "seek", "x"],
            },
        })
        .to_string()
    }

    fn artwork_message(id: &str, url: &str, bytes: usize) -> String {
        let data = format!("data:image/png;base64,{}", "A".repeat(bytes - 22));
        serde_json::json!({ "type": "artwork", "id": id, "url": url, "data": data }).to_string()
    }

    #[test]
    fn bounds_what_one_connection_holds() {
        let mut tabs = Tabs::default();
        assert!(tabs.handle(&player_message("1:0", &"ü".repeat(MAX_TEXT_BYTES), "u")));
        let state = &tabs.tabs["1:0"].state;
        assert!(state.title.len() <= MAX_TEXT_BYTES && state.title.chars().all(|c| c == 'ü'));
        assert_eq!(state.actions, ["play", "pause", "seek"]);
        assert!(!tabs.handle(&player_message(&"1".repeat(MAX_TEXT_BYTES + 1), "Long id", "u")));
        tabs.handle(&player_message("1:1", "Long cover URL", &"u".repeat(MAX_TEXT_BYTES + 1)));
        assert_eq!(tabs.tabs["1:1"].state.artwork_url, None);

        // Two tabs are there already.
        for i in 2..MAX_TABS {
            assert!(tabs.handle(&player_message(&format!("2:{i}"), "Song", "u")));
        }
        assert!(!tabs.handle(&player_message("3:0", "One too many", "u")));
        assert!(tabs.handle(&player_message("2:2", "A known tab still changes", "u")));

        assert!(!tabs.handle(&artwork_message("1:0", "u", MAX_ARTWORK_BYTES + 1)));
        let per_tab = MAX_ARTWORK_BYTES;
        let fit = MAX_ARTWORK_TOTAL / per_tab;
        for i in 0..fit {
            assert!(tabs.handle(&artwork_message(&format!("2:{}", i + 2), "u", per_tab)), "cover {i}");
        }
        assert!(!tabs.handle(&artwork_message(&format!("2:{}", fit + 2), "u", per_tab)));
        // Replacing a tab's own cover does not count the old one.
        assert!(tabs.handle(&artwork_message("2:2", "u", per_tab)));
    }

    #[test]
    fn limits_open_connections() {
        let open = Arc::new(AtomicUsize::new(0));
        let slots: Vec<Slot> = (0..MAX_CONNECTIONS).map(|_| Slot::take(&open).unwrap()).collect();
        assert!(Slot::take(&open).is_none());
        assert_eq!(open.load(Ordering::SeqCst), MAX_CONNECTIONS);
        drop(slots);
        assert_eq!(open.load(Ordering::SeqCst), 0);
        assert!(Slot::take(&open).is_some());
    }
}
