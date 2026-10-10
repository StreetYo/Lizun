//! Clipboard history (see ADR 0008): text, pictures and files the user copies, shown in the
//! island's Clipboard tab, where a click puts an entry back on the clipboard. Files are kept as
//! their paths; the files themselves stay where they are.
//!
//! A background thread checks the clipboard's change counter twice a second and records what
//! is new. The history is kept in the app data folder, `history.json` with a PNG file per
//! picture, so it survives a restart; entries go once they are older than the lifetime set in
//! the settings or pushed out by the limit. Clips that apps mark as private, such as passwords
//! from a password manager, are never read.

use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::preferences::Preferences;

/// Sent to every window with the entries, as [`EntryView`]s, whenever the history changes.
pub const EVENT_CHANGED: &str = "clipboard://changed";
const DIR_NAME: &str = "clipboard";
const INDEX_FILE: &str = "history.json";
const POLL: Duration = Duration::from_millis(500);
/// How often entries are checked against the lifetime.
const EXPIRE_EVERY: Duration = Duration::from_secs(30);
/// Longer texts, such as a whole log file, are not kept.
const MAX_TEXT_BYTES: usize = 1024 * 1024;
/// Larger pictures are not kept: 40 megapixels is more than an 8K screenshot.
pub const MAX_PIXELS: u64 = 40_000_000;
/// The island shows the start of a text; the whole text is copied.
const PREVIEW_CHARS: usize = 300;
/// Copies of more files, such as a whole photo library, are not kept.
const MAX_FILES: usize = 10_000;
/// The island shows the first few files of a copy as a grid.
const PREVIEW_FILES: usize = 4;
/// Files whose preview the island shows through the asset protocol, as in the file tray.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif"];

/// Something on the clipboard.
pub enum Clip {
    Text(String),
    Picture(Picture),
    /// Paths of files and folders copied in Explorer or another app.
    Files(Vec<String>),
}

/// Which kinds of clips the history keeps. A clip counts as its first kind in the order files,
/// text, picture, so files that come with their paths as text are not kept as text when files
/// are turned off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kinds {
    pub text: bool,
    pub pictures: bool,
    pub files: bool,
}

/// 8-bit RGBA pixels, row by row from the top.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: u64,
    /// When it was copied (last, if it was copied more than once), in ms since the Unix epoch.
    copied_at: i64,
    #[serde(flatten)]
    content: Content,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Content {
    Text { text: String },
    /// Kept as `<id>.png` in the history folder; the hash tells copies of the same picture.
    Picture { width: u32, height: u32, hash: u64 },
    Files { paths: Vec<String> },
}

impl Content {
    fn is_allowed(&self, kinds: Kinds) -> bool {
        match self {
            Content::Text { .. } => kinds.text,
            Content::Picture { .. } => kinds.pictures,
            Content::Files { .. } => kinds.files,
        }
    }

    fn is_copy_of(&self, clip: &Clip, hash: u64) -> bool {
        match (self, clip) {
            (Content::Text { text }, Clip::Text(new)) => text == new,
            (Content::Picture { width, height, hash: old }, Clip::Picture(new)) => {
                (*width, *height, *old) == (new.width, new.height, hash)
            }
            (Content::Files { paths }, Clip::Files(new)) => paths == new,
            _ => false,
        }
    }
}

/// An entry as the island shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    id: u64,
    copied_at: i64,
    #[serde(flatten)]
    preview: Preview,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Preview {
    /// The start of the text; `cut` when there is more.
    Text { text: String, cut: bool },
    /// The PNG file, served through the asset protocol.
    Picture { path: String, width: u32, height: u32 },
    /// The first few files and how many there are in all.
    Files { files: Vec<FileView>, count: usize },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileView {
    path: String,
    folder: bool,
    /// Moved or deleted since it was copied. Files on network shares are not checked.
    missing: bool,
}

/// What the settings ask of the history.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rules {
    enabled: bool,
    kinds: Kinds,
    lifetime: Option<Duration>,
    limit: Option<usize>,
}

impl Rules {
    fn of(preferences: &Preferences) -> Self {
        let hours = u64::from(preferences.clipboard_lifetime_hours);
        Self {
            enabled: preferences.clipboard_history,
            kinds: Kinds {
                text: preferences.clipboard_text,
                pictures: preferences.clipboard_pictures,
                files: preferences.clipboard_files,
            },
            lifetime: (hours > 0).then(|| Duration::from_secs(hours * 3600)),
            limit: preferences.clipboard_limit_enabled.then_some(preferences.clipboard_limit as usize),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Index {
    next_id: u64,
    /// Newest first.
    entries: Vec<Entry>,
}

pub struct History {
    state: Mutex<State>,
    wake: Sender<()>,
}

struct State {
    index: Index,
    rules: Rules,
    /// None when the app data folder is unknown; the history then lives only in memory.
    dir: Option<PathBuf>,
}

pub fn start(app: &AppHandle) {
    let dir = app.path().app_data_dir().ok().map(|dir| dir.join(DIR_NAME));
    let mut index = dir.as_deref().map(load).unwrap_or_default();
    if let Some(dir) = &dir {
        remove_strays(dir, &index);
        for entry in &index.entries {
            if matches!(entry.content, Content::Picture { .. }) {
                allow_picture(app, dir, entry.id);
            }
        }
    }
    index.next_id = index.next_id.max(index.entries.iter().map(|entry| entry.id + 1).max().unwrap_or(0));
    index.entries.iter().for_each(|entry| allow_previews(app, &entry.content));
    let rules = Rules::of(&crate::preferences::current(app));
    let mut state = State { index, rules, dir };
    if state.apply_rules() {
        state.save();
    }
    let (wake, wakes) = mpsc::channel();
    app.manage(History { state: Mutex::new(state), wake });
    let handle = app.clone();
    if let Err(err) = thread::Builder::new().name("lizun-clipboard".into()).spawn(move || run(&handle, wakes)) {
        log::error!("failed to start the clipboard thread: {err}");
    }
}

/// Called after the preferences change: a shorter lifetime or a lower limit applies at once,
/// and turning the history or a kind of clip off deletes those entries.
pub fn configure(app: &AppHandle, preferences: &Preferences) {
    let Some(history) = app.try_state::<History>() else { return };
    let changed = {
        let mut state = history.state.lock().unwrap();
        state.rules = Rules::of(preferences);
        let changed = state.apply_rules();
        if changed {
            state.save();
        }
        changed
    };
    if changed {
        announce(app);
    }
    let _ = history.wake.send(());
}

fn run(app: &AppHandle, wakes: Receiver<()>) {
    let mut seen = None;
    let mut last_expiry = Instant::now();
    loop {
        match wakes.recv_timeout(POLL) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let history = app.state::<History>();
        if last_expiry.elapsed() >= EXPIRE_EVERY {
            last_expiry = Instant::now();
            let expired = {
                let mut state = history.state.lock().unwrap();
                let expired = state.apply_rules();
                if expired {
                    state.save();
                }
                expired
            };
            if expired {
                announce(app);
            }
        }

        let sequence = crate::platform::clipboard_sequence();
        if seen == Some(sequence) {
            continue;
        }
        let rules = history.state.lock().unwrap().rules;
        if !rules.enabled {
            // What was copied while the history was off is not picked up later.
            seen = Some(sequence);
            continue;
        }
        // While another app has the clipboard open, try again on the next tick.
        let Ok(clip) = crate::platform::read_clipboard(rules.kinds) else { continue };
        seen = Some(sequence);
        if let Some(clip) = clip {
            record(app, &history, clip);
        }
    }
}

/// Puts a clip at the top of the history. Copying the newest entry again changes nothing; an
/// older one moves to the top.
fn record(app: &AppHandle, history: &History, clip: Clip) {
    let skipped = match &clip {
        Clip::Text(text) => text.trim().is_empty() || text.len() > MAX_TEXT_BYTES,
        Clip::Files(paths) => paths.len() > MAX_FILES,
        Clip::Picture(_) => false,
    };
    if skipped {
        return;
    }
    let hash = match &clip {
        Clip::Picture(picture) => fnv1a(&picture.rgba),
        Clip::Text(_) | Clip::Files(_) => 0,
    };
    let id = {
        let mut state = history.state.lock().unwrap();
        let entries = &mut state.index.entries;
        match entries.iter().position(|entry| entry.content.is_copy_of(&clip, hash)) {
            Some(0) => return,
            Some(at) => {
                let mut entry = entries.remove(at);
                entry.copied_at = now_ms();
                entries.insert(0, entry);
                state.save();
                drop(state);
                announce(app);
                return;
            }
            None => {
                state.index.next_id += 1;
                state.index.next_id - 1
            }
        }
    };

    // Pictures are written to their file before the entry appears, outside the lock.
    let content = match clip {
        Clip::Text(text) => Content::Text { text },
        Clip::Files(paths) => Content::Files { paths },
        Clip::Picture(picture) => {
            let dir = history.state.lock().unwrap().dir.clone();
            if let Some(dir) = dir {
                let written = encode_png(&picture).and_then(|png| {
                    fs::create_dir_all(&dir)
                        .and_then(|()| fs::write(picture_path(&dir, id), png))
                        .map_err(|e| e.to_string())
                });
                if let Err(err) = written {
                    log::warn!("could not keep a copied picture: {err}");
                    return;
                }
                allow_picture(app, &dir, id);
            }
            Content::Picture { width: picture.width, height: picture.height, hash }
        }
    };
    allow_previews(app, &content);
    {
        let mut state = history.state.lock().unwrap();
        state.index.entries.insert(0, Entry { id, copied_at: now_ms(), content });
        // Also deletes the new entry if the history was turned off while its picture was written.
        state.apply_rules();
        state.save();
    }
    announce(app);
}

impl State {
    /// Drops what the rules no longer allow. True when something was dropped.
    fn apply_rules(&mut self) -> bool {
        let now = now_ms();
        let before = self.index.entries.len();
        let rules = self.rules;
        let mut dropped = Vec::new();
        let mut kept = Vec::with_capacity(before);
        for (at, entry) in std::mem::take(&mut self.index.entries).into_iter().enumerate() {
            let expired = rules.lifetime.is_some_and(|lifetime| now - entry.copied_at > lifetime.as_millis() as i64);
            let over_limit = rules.limit.is_some_and(|limit| at >= limit);
            if !rules.enabled || !entry.content.is_allowed(rules.kinds) || expired || over_limit {
                dropped.push(entry);
            } else {
                kept.push(entry);
            }
        }
        self.index.entries = kept;
        self.remove_files(&dropped);
        !dropped.is_empty()
    }

    fn remove_files(&self, entries: &[Entry]) {
        let Some(dir) = &self.dir else { return };
        for entry in entries {
            if matches!(entry.content, Content::Picture { .. }) {
                let _ = fs::remove_file(picture_path(dir, entry.id));
            }
        }
    }

    fn save(&self) {
        let Some(dir) = &self.dir else { return };
        let written = serde_json::to_vec(&self.index).map_err(|e| e.to_string()).and_then(|json| {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            // Written next to the index and swapped in, so a crash never leaves half a file.
            let temporary = dir.join(format!("{INDEX_FILE}.tmp"));
            fs::write(&temporary, json).map_err(|e| e.to_string())?;
            fs::rename(&temporary, dir.join(INDEX_FILE)).map_err(|e| e.to_string())
        });
        if let Err(err) = written {
            log::warn!("could not save the clipboard history: {err}");
        }
    }

    /// The entries as the island shows them, before [`check_files`].
    fn views(&self) -> Vec<EntryView> {
        self.index.entries.iter().map(|entry| self.view(entry)).collect()
    }

    fn view(&self, entry: &Entry) -> EntryView {
        let preview = match &entry.content {
            Content::Text { text } => {
                let mut chars = text.char_indices();
                let cut = chars.nth(PREVIEW_CHARS).map(|(at, _)| at);
                Preview::Text { text: text[..cut.unwrap_or(text.len())].to_string(), cut: cut.is_some() }
            }
            Content::Picture { width, height, .. } => Preview::Picture {
                path: self.dir.as_deref().map(|dir| picture_path(dir, entry.id).display().to_string()).unwrap_or_default(),
                width: *width,
                height: *height,
            },
            Content::Files { paths } => Preview::Files {
                files: paths
                    .iter()
                    .take(PREVIEW_FILES)
                    .map(|path| FileView { path: path.clone(), folder: false, missing: false })
                    .collect(),
                count: paths.len(),
            },
        };
        EntryView { id: entry.id, copied_at: entry.copied_at, preview }
    }
}

/// Fills in which files are folders and which are gone. Done outside the history's lock, since
/// a slow disk keeps it waiting.
fn check_files(views: &mut [EntryView]) {
    for view in views {
        let Preview::Files { files, .. } = &mut view.preview else { continue };
        for file in files.iter_mut().filter(|file| !is_network_path(&file.path)) {
            let metadata = fs::metadata(&file.path);
            file.folder = metadata.as_ref().is_ok_and(|metadata| metadata.is_dir());
            file.missing = metadata.is_err();
        }
    }
}

/// A path on another computer, such as `\\server\share\file`. Lizun does not look at those on
/// its own: a slow or offline share blocks for many seconds, and opening one signs in to that
/// server with the user's Windows credentials.
fn is_network_path(path: &str) -> bool {
    let path = path.replace('/', "\\");
    match path.strip_prefix(r"\\?\").or_else(|| path.strip_prefix(r"\\.\")) {
        Some(rest) => rest.get(..4).is_some_and(|unc| unc.eq_ignore_ascii_case(r"UNC\")),
        None => path.starts_with(r"\\"),
    }
}

fn announce(app: &AppHandle) {
    let mut views = app.state::<History>().state.lock().unwrap().views();
    check_files(&mut views);
    if let Err(err) = app.emit(EVENT_CHANGED, views) {
        log::warn!("failed to announce the clipboard history: {err}");
    }
}

fn load(dir: &Path) -> Index {
    let Ok(text) = fs::read_to_string(dir.join(INDEX_FILE)) else { return Index::default() };
    let mut index: Index = serde_json::from_str(&text).unwrap_or_else(|err| {
        log::warn!("ignoring an unreadable clipboard history: {err}");
        Index::default()
    });
    // A picture whose file is gone cannot be shown or copied.
    index.entries.retain(|entry| match entry.content {
        Content::Picture { .. } => picture_path(dir, entry.id).is_file(),
        Content::Text { .. } | Content::Files { .. } => true,
    });
    index
}

/// Deletes pictures no entry refers to, left behind by a crash.
fn remove_strays(dir: &Path, index: &Index) {
    let Ok(files) = fs::read_dir(dir) else { return };
    let known: HashSet<String> = index.entries.iter().map(|entry| format!("{}.png", entry.id)).collect();
    for file in files.flatten() {
        let name = file.file_name().to_string_lossy().into_owned();
        if name.ends_with(".png") && !known.contains(&name) {
            let _ = fs::remove_file(file.path());
        }
    }
}

/// Lets the island show a picture of the history. Only the pictures are allowed, not the folder,
/// which also holds every copied text.
fn allow_picture(app: &AppHandle, dir: &Path, id: u64) {
    if let Err(err) = app.asset_protocol_scope().allow_file(picture_path(dir, id)) {
        log::warn!("a picture in the clipboard history cannot be shown: {err}");
    }
}

/// Lets the island show the pictures among the first files of a copy, except on network shares.
fn allow_previews(app: &AppHandle, content: &Content) {
    let Content::Files { paths } = content else { return };
    let scope = app.asset_protocol_scope();
    for path in paths.iter().take(PREVIEW_FILES).filter(|path| !is_network_path(path)) {
        let extension = Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if IMAGE_EXTENSIONS.contains(&extension.as_str())
            && let Err(err) = scope.allow_file(path)
        {
            log::warn!("cannot preview a copied file: {err}");
        }
    }
}

fn picture_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.png"))
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as i64).unwrap_or(0)
}

/// A hash that stays the same across builds, since it is saved with the history.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

fn encode_png(picture: &Picture) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, picture.width, picture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&picture.rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(png)
}

/// Reads a PNG of any colour type into RGBA. None for files that are broken or too large.
pub fn decode_png(bytes: &[u8]) -> Option<Picture> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width, reader.info().height);
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return None;
    }
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Picture { width, height, rgba })
}

// Off the main thread, since checking the files can be slow.
#[tauri::command(async)]
pub fn clipboard_history(history: tauri::State<'_, History>) -> Vec<EntryView> {
    let mut views = history.state.lock().unwrap().views();
    check_files(&mut views);
    views
}

/// Puts an entry back on the clipboard. The watcher then sees it and moves it to the top.
/// Files go back as a copy, even if they were cut, and only those that are still there.
#[tauri::command]
pub async fn clipboard_copy(app: AppHandle, window: WebviewWindow, id: u64) -> Result<(), String> {
    let history = app.state::<History>();
    let (content, dir) = {
        let state = history.state.lock().unwrap();
        let entry = state.index.entries.iter().find(|entry| entry.id == id).ok_or("no such entry")?;
        (entry.content.clone(), state.dir.clone())
    };
    match content {
        Content::Text { text } => crate::platform::write_clipboard_text(&window, &text),
        Content::Files { paths } => {
            let present: Vec<String> =
                paths.into_iter().filter(|path| is_network_path(path) || Path::new(path).exists()).collect();
            if present.is_empty() {
                return Err("the files are gone".into());
            }
            crate::platform::write_clipboard_files(&window, &present)
        }
        Content::Picture { .. } => {
            let path = picture_path(&dir.ok_or("no history folder")?, id);
            let png = fs::read(&path).map_err(|e| e.to_string())?;
            let picture = decode_png(&png).ok_or("the picture cannot be read")?;
            crate::platform::write_clipboard_picture(&window, &picture, &png)
        }
    }
}

#[tauri::command(async)]
pub fn clipboard_remove(app: AppHandle, id: u64) {
    {
        let history = app.state::<History>();
        let mut state = history.state.lock().unwrap();
        let Some(at) = state.index.entries.iter().position(|entry| entry.id == id) else { return };
        let entry = state.index.entries.remove(at);
        state.remove_files(&[entry]);
        state.save();
    }
    announce(&app);
}

#[tauri::command(async)]
pub fn clipboard_clear(app: AppHandle) {
    {
        let history = app.state::<History>();
        let mut state = history.state.lock().unwrap();
        let entries = std::mem::take(&mut state.index.entries);
        state.remove_files(&entries);
        state.save();
    }
    announce(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(id: u64, copied_at: i64) -> Entry {
        Entry { id, copied_at, content: Content::Text { text: format!("text {id}") } }
    }

    const ALL: Kinds = Kinds { text: true, pictures: true, files: true };

    fn state(entries: Vec<Entry>, rules: Rules) -> State {
        State { index: Index { next_id: 100, entries }, rules, dir: None }
    }

    #[test]
    fn drops_old_entries_and_those_over_the_limit() {
        let now = now_ms();
        let hour = 3_600_000;
        let entries = vec![text(3, now), text(2, now - hour / 2), text(1, now - 2 * hour)];
        let mut by_age = state(
            entries.clone(),
            Rules { enabled: true, kinds: ALL, lifetime: Some(Duration::from_secs(3600)), limit: None },
        );
        assert!(by_age.apply_rules());
        assert_eq!(by_age.index.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [3, 2]);

        let mut by_count = state(entries.clone(), Rules { enabled: true, kinds: ALL, lifetime: None, limit: Some(1) });
        assert!(by_count.apply_rules());
        assert_eq!(by_count.index.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [3]);

        let mut unlimited = state(entries.clone(), Rules { enabled: true, kinds: ALL, lifetime: None, limit: None });
        assert!(!unlimited.apply_rules());
        assert_eq!(unlimited.index.entries.len(), 3);

        let mut off = state(entries, Rules { enabled: false, kinds: ALL, lifetime: None, limit: None });
        assert!(off.apply_rules());
        assert!(off.index.entries.is_empty());
    }

    #[test]
    fn drops_the_kinds_turned_off() {
        let files = Entry { id: 2, copied_at: now_ms(), content: Content::Files { paths: vec!["C:\\a".into()] } };
        let entries = vec![files, text(1, now_ms())];
        let kinds = Kinds { files: false, ..ALL };
        let mut no_files = state(entries, Rules { enabled: true, kinds, lifetime: None, limit: None });
        assert!(no_files.apply_rules());
        assert_eq!(no_files.index.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn previews_the_start_of_long_texts() {
        let long = "ё".repeat(PREVIEW_CHARS + 5);
        let state = state(vec![], Rules { enabled: true, kinds: ALL, lifetime: None, limit: None });
        let view = state.view(&Entry { id: 1, copied_at: 0, content: Content::Text { text: long } });
        let Preview::Text { text, cut } = view.preview else { panic!("a text") };
        assert!(cut);
        assert_eq!(text.chars().count(), PREVIEW_CHARS);
    }

    #[test]
    fn previews_the_first_files() {
        let existing = std::env::temp_dir().display().to_string();
        let paths = vec![existing.clone(), "Z:\\gone\\a.txt".into(), r"\\server\share\a.png".into(), "c".into(), "d".into()];
        let state = state(vec![], Rules { enabled: true, kinds: ALL, lifetime: None, limit: None });
        let mut views = vec![state.view(&Entry { id: 1, copied_at: 0, content: Content::Files { paths } })];
        check_files(&mut views);
        let Preview::Files { files, count } = &views[0].preview else { panic!("files") };
        assert_eq!((files.len(), *count), (PREVIEW_FILES, 5));
        assert!(files[0].folder && !files[0].missing);
        assert!(!files[1].folder && files[1].missing);
        // Not looked at, so not reported gone either.
        assert!(!files[2].folder && !files[2].missing);
    }

    #[test]
    fn knows_network_paths() {
        for path in [r"\\server\share\a.png", "//server/share/a.png", r"\\?\UNC\server\share\a.png", r"\\?\unc\server\a"] {
            assert!(is_network_path(path), "{path}");
        }
        for path in [r"C:\Users\me\a.png", r"\\?\C:\Users\me\a.png", r"\\.\C:\a.png", "relative.txt", ""] {
            assert!(!is_network_path(path), "{path}");
        }
    }

    #[test]
    fn pictures_survive_png() {
        let picture = Picture { width: 2, height: 1, rgba: vec![1, 2, 3, 4, 250, 251, 252, 0] };
        let back = decode_png(&encode_png(&picture).unwrap()).unwrap();
        assert_eq!((back.width, back.height, back.rgba), (2, 1, picture.rgba));
    }

    #[test]
    fn index_format() {
        let index = Index {
            next_id: 3,
            entries: vec![
                Entry { id: 2, copied_at: 5, content: Content::Picture { width: 1, height: 2, hash: 7 } },
                Entry { id: 1, copied_at: 4, content: Content::Text { text: "hi".into() } },
            ],
        };
        let json = serde_json::to_string(&index).unwrap();
        assert_eq!(
            json,
            r#"{"nextId":3,"entries":[{"id":2,"copiedAt":5,"kind":"picture","width":1,"height":2,"hash":7},{"id":1,"copiedAt":4,"kind":"text","text":"hi"}]}"#
        );
        let back: Index = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries, index.entries);
    }
}
