//! Installing and removing the user's widgets: from a GitHub link, a zip archive or a folder.
//!
//! Installing takes two steps. `widgets_install_prepare` downloads or copies the widget into a
//! staging folder and validates it there, then returns its manifest so the settings window can
//! show what the widget asks for. `widgets_install_confirm` moves it into the widgets folder,
//! replacing an installed widget with the same id. A widget reaches the widgets folder only
//! after it passed validation and the user agreed.
//!
//! Lizun records where each widget was installed from. A widget from another source that uses
//! an id Lizun has seen before is shown with a warning, and its settings start from the defaults,
//! so it cannot take over another widget's place and read what the user entered for that one.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};
use zip::ZipArchive;

use crate::i18n::{t, tf};
use crate::widgets::{self, InstalledWidget, MANIFEST_FILE, WidgetManifest};

/// Largest archive Lizun downloads or opens.
const MAX_ARCHIVE_BYTES: u64 = 50 * 1024 * 1024;
/// Limits for the widget once unpacked, so an archive cannot fill the disk.
const MAX_WIDGET_BYTES: u64 = 50 * 1024 * 1024;
const MAX_WIDGET_FILES: usize = 2_000;
/// Folder in the app data folder for widgets on their way in or out. It is on the same drive
/// as the widgets folder, so moving between them is a rename.
const STAGING_DIR: &str = "widget-staging";
/// File in the app data folder with the source of every widget installed through Lizun.
const SOURCES_FILE: &str = "widget-sources.json";
/// Source of a widget installed from a zip archive or a folder on this computer.
const LOCAL_SOURCE: &str = "local";

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstallSource {
    /// A link to a GitHub repository, a folder or release in one, or a zip file on GitHub.
    Github { url: String },
    /// A zip archive or a widget folder on this computer.
    Path { path: PathBuf },
}

/// A widget that was unpacked and validated and waits for the user to confirm.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingInstall {
    token: u64,
    manifest: WidgetManifest,
    /// Where the widget comes from, for the user to recognise it.
    origin: String,
    /// Version of the installed widget this one replaces.
    replaces: Option<String>,
    /// Where a widget with this id was installed from before, if Lizun knows.
    previous_source: Option<String>,
    /// The widget comes from another source than the one Lizun had with its id, or Lizun had the
    /// id but does not know its source; its settings are reset when it is installed.
    source_changed: bool,
}

struct Staged {
    token: u64,
    /// `<staging>/<token>`; the widget itself is in its `widget` subfolder.
    dir: PathBuf,
    manifest: WidgetManifest,
    source: String,
    source_changed: bool,
}

impl Staged {
    fn discard(self) {
        remove_quietly(&self.dir);
    }
}

#[derive(Default)]
struct Installer {
    pending: Mutex<Option<Staged>>,
    next_token: AtomicU64,
    /// Where each widget was installed from, by id: `github.com/<owner>/<repo>` or `local`.
    /// Entries stay when a widget is removed, like its settings, so a reinstall from somewhere
    /// else is noticed too.
    sources: Mutex<BTreeMap<String, String>>,
}

pub fn init(app: &AppHandle) {
    let sources = sources_path(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| {
            serde_json::from_str(&text).unwrap_or_else(|err| {
                log::warn!("ignoring unreadable {SOURCES_FILE}: {err}");
                BTreeMap::new()
            })
        })
        .unwrap_or_default();
    app.manage(Installer { sources: Mutex::new(sources), ..Installer::default() });
    // Leftovers of installs that were never confirmed, or of a removal that was interrupted.
    if let Some(staging) = staging_root(app) {
        remove_quietly(&staging);
    }
}

#[tauri::command]
pub async fn widgets_install_prepare(app: AppHandle, source: InstallSource) -> Result<PendingInstall, String> {
    let installer = app.state::<Installer>();
    let token = installer.next_token.fetch_add(1, Ordering::Relaxed) + 1;
    let dir = staging_root(&app).ok_or("no app data folder")?.join(token.to_string());

    let staging = dir.clone();
    let (manifest, origin, source) = tauri::async_runtime::spawn_blocking(move || stage(&source, &staging))
        .await
        .map_err(|e| e.to_string())?
        .inspect_err(|_| remove_quietly(&dir))?;

    let replaces = match widgets::lookup(&app, &manifest.id) {
        None => None,
        Some(installed) if installed.source == "user" => Some(installed.manifest.version),
        Some(installed) => {
            remove_quietly(&dir);
            let key = if installed.source == "bundled" { "install.idTakenBundled" } else { "install.idTakenDev" };
            return Err(tf(key, &[("id", &manifest.id), ("name", &installed.manifest.name)]));
        }
    };

    let previous_source = installer.sources.lock().unwrap().get(&manifest.id).cloned();
    let seen_before = replaces.is_some() || crate::widget_settings::has_values(&app, &manifest.id);
    let source_changed = source_changed(previous_source.as_deref(), &source, seen_before);

    let staged = Staged { token, dir, manifest: manifest.clone(), source, source_changed };
    if let Some(previous) = installer.pending.lock().unwrap().replace(staged) {
        previous.discard();
    }
    Ok(PendingInstall { token, manifest, origin, replaces, previous_source, source_changed })
}

/// Whether a widget from `source` differs from what Lizun had under its id: another recorded
/// source, or none recorded for an id that is installed or has settings (a widget copied in by
/// hand, or installed before sources were recorded).
fn source_changed(recorded: Option<&str>, source: &str, seen_before: bool) -> bool {
    match recorded {
        Some(recorded) => recorded != source,
        None => seen_before,
    }
}

#[tauri::command]
pub async fn widgets_install_confirm(app: AppHandle, token: u64) -> Result<Vec<InstalledWidget>, String> {
    let staged = take_pending(&app, token).ok_or_else(|| t("install.notWaiting"))?;
    // The settings go before the widget comes in, so it never sees the earlier widget's values.
    // The source is recorded last: if anything fails, the next install of this id still counts
    // as a change.
    if staged.source_changed {
        crate::widget_settings::forget(&app, &staged.manifest.id, &staged.manifest.settings)?;
    }
    let (id, source) = (staged.manifest.id.clone(), staged.source.clone());
    let target_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || install(&target_app, staged))
        .await
        .map_err(|e| e.to_string())??;
    let widgets = widgets::reload(&app);
    record_source(&app, &id, source)?;
    Ok(widgets)
}

#[tauri::command]
pub fn widgets_install_cancel(app: AppHandle, token: u64) {
    if let Some(staged) = take_pending(&app, token) {
        staged.discard();
    }
}

/// Removes a widget the user installed. Its settings and stored data are kept, so installing
/// it again brings them back.
#[tauri::command]
pub async fn widgets_uninstall(app: AppHandle, id: String) -> Result<Vec<InstalledWidget>, String> {
    let widget = widgets::lookup(&app, &id).ok_or_else(|| t("install.notInstalled"))?;
    if widget.source != "user" {
        return Err(t("install.onlyUser"));
    }
    let token = app.state::<Installer>().next_token.fetch_add(1, Ordering::Relaxed) + 1;
    let trash = staging_root(&app).ok_or("no app data folder")?.join(format!("removed-{token}"));
    tauri::async_runtime::spawn_blocking(move || {
        // Moved out first, so an interrupted delete cannot leave half a widget behind.
        fs::create_dir_all(trash.parent().unwrap_or(&trash)).map_err(|e| e.to_string())?;
        fs::rename(&widget.dir, &trash).map_err(|e| folder_in_use(&e))?;
        remove_quietly(&trash);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(widgets::reload(&app))
}

fn record_source(app: &AppHandle, id: &str, source: String) -> Result<(), String> {
    let installer = app.state::<Installer>();
    let mut sources = installer.sources.lock().unwrap();
    let mut next = sources.clone();
    next.insert(id.to_string(), source);
    let path = sources_path(app).ok_or("no app data folder")?;
    let json = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    *sources = next;
    Ok(())
}

fn sources_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join(SOURCES_FILE))
}

fn take_pending<R: Runtime>(app: &AppHandle<R>, token: u64) -> Option<Staged> {
    let installer = app.state::<Installer>();
    let mut pending = installer.pending.lock().unwrap();
    if pending.as_ref().is_some_and(|staged| staged.token == token) { pending.take() } else { None }
}

fn staging_root<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join(STAGING_DIR))
}

/// Puts the widget from `source` into `<dir>/widget` and validates it there. Returns its
/// manifest, the link or path it came from for display, and its source for the record.
fn stage(source: &InstallSource, dir: &Path) -> Result<(WidgetManifest, String, String), String> {
    let widget_dir = dir.join("widget");
    fs::create_dir_all(&widget_dir).map_err(|e| e.to_string())?;
    let mut recorded = LOCAL_SOURCE.to_string();
    let origin = match source {
        InstallSource::Github { url } => {
            let link = GitHubLink::parse(url)?;
            let archive = download(&link.archive_url())?;
            unpack(&archive, link.folder.as_deref(), &widget_dir)?;
            recorded = link.source();
            link.origin
        }
        InstallSource::Path { path } if path.is_dir() => {
            if !path.join(MANIFEST_FILE).is_file() {
                return Err(t("install.folderWithoutManifest"));
            }
            copy_folder(path, &widget_dir, &mut Budget::default())?;
            path.display().to_string()
        }
        InstallSource::Path { path } if is_zip(path) => {
            unpack(&read_archive(path)?, None, &widget_dir)?;
            path.display().to_string()
        }
        InstallSource::Path { .. } => return Err(t("install.zipOrFolder")),
    };
    let manifest = widgets::load_manifest(&widget_dir).map_err(|err| tf("install.invalidManifest", &[("error", &err)]))?;
    if !widget_dir.join(&manifest.entry).is_file() {
        return Err(tf("install.noEntry", &[("entry", &manifest.entry)]));
    }
    Ok((manifest, origin, recorded))
}

/// Moves a staged widget into `<widgets>/<id>`, replacing the installed copy if there is one.
fn install<R: Runtime>(app: &AppHandle<R>, staged: Staged) -> Result<(), String> {
    let widgets_dir = widgets::user_widgets_dir(app).ok_or("no widgets folder")?;
    fs::create_dir_all(&widgets_dir).map_err(|e| e.to_string())?;
    let target = widgets_dir.join(&staged.manifest.id);

    // The copy being replaced may sit in a folder with another name. A folder that already
    // has the target's name but holds no valid widget is moved out of the way as well.
    let installed = widgets::lookup(app, &staged.manifest.id).filter(|w| w.source == "user").map(|w| w.dir);
    let mut in_the_way: Vec<PathBuf> = installed.into_iter().collect();
    if target.exists() && !in_the_way.contains(&target) {
        in_the_way.push(target.clone());
    }
    let result = move_into_place(&staged.dir, &target, &in_the_way);
    staged.discard();
    result
}

/// Moves `<staging>/widget` to `target` after moving `in_the_way` into the staging folder.
/// When a step fails, the folders that were moved go back where they were.
fn move_into_place(staging: &Path, target: &Path, in_the_way: &[PathBuf]) -> Result<(), String> {
    let mut moved: Vec<(&Path, PathBuf)> = Vec::new();
    let result = (|| {
        for (index, dir) in in_the_way.iter().enumerate() {
            let aside = staging.join(format!("replaced-{index}"));
            fs::rename(dir, &aside).map_err(|e| folder_in_use(&e))?;
            moved.push((dir, aside));
        }
        fs::rename(staging.join("widget"), target).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        for (original, aside) in moved.iter().rev() {
            if let Err(err) = fs::rename(aside, original) {
                log::error!("could not put {} back: {err}", original.display());
            }
        }
    }
    result
}

fn folder_in_use(err: &io::Error) -> String {
    tf("install.folderInUse", &[("error", err)])
}

fn remove_quietly(dir: &Path) {
    if dir.exists()
        && let Err(err) = fs::remove_dir_all(dir)
    {
        log::warn!("could not remove {}: {err}", dir.display());
    }
}

/// A GitHub link the user pasted, reduced to what is needed to download the widget.
#[derive(Debug, PartialEq)]
struct GitHubLink {
    owner: String,
    repo: String,
    target: LinkTarget,
    /// The link without the scheme, for display.
    origin: String,
    /// Folder of the widget inside the repository, from a `/tree/<ref>/<folder>` link.
    folder: Option<String>,
}

#[derive(Debug, PartialEq)]
enum LinkTarget {
    /// A branch, tag or commit; `None` means the default branch.
    Ref(Option<String>),
    /// A zip file served by github.com, such as a release asset; the path after the repository.
    File(String),
}

const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');

impl GitHubLink {
    /// Accepts the links people copy from GitHub:
    ///
    /// - `github.com/<owner>/<repo>`: the default branch;
    /// - `.../tree/<ref>` and `.../tree/<ref>/<folder>`: a branch, tag or commit, and a folder in
    ///   it for repositories with several widgets (a branch name with a slash is not supported);
    /// - `.../blob/<ref>/<folder>/manifest.json`: the same, linked through the manifest;
    /// - `.../releases/tag/<tag>`: the source of a release;
    /// - `.../releases/download/<tag>/<file>.zip`, `.../releases/latest/download/<file>.zip`
    ///   and `.../archive/<...>.zip`: a zip file.
    fn parse(input: &str) -> Result<Self, String> {
        let usage = || t("install.usage");
        let link = input.trim();
        let link = link.split(['?', '#']).next().unwrap_or_default();
        let link = link.strip_prefix("https://").or_else(|| link.strip_prefix("http://")).unwrap_or(link);
        let link = link.strip_prefix("www.").unwrap_or(link);
        let path = link.strip_prefix("github.com/").ok_or_else(usage)?;
        let segments: Vec<String> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| percent_decode_str(segment).decode_utf8_lossy().into_owned())
            .collect();
        let [owner, repo, rest @ ..] = segments.as_slice() else { return Err(usage()) };
        let repo = repo.strip_suffix(".git").unwrap_or(repo);
        let name_ok = |name: &str| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
        if !name_ok(owner) || !name_ok(repo) {
            return Err(usage());
        }
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        let is_zip = rest.last().is_some_and(|file| file.to_ascii_lowercase().ends_with(".zip"));
        let (target, folder) = match rest.as_slice() {
            [] => (LinkTarget::Ref(None), None),
            ["tree", reference, folder @ ..] => (LinkTarget::Ref(Some(reference.to_string())), join(folder)),
            ["blob", reference, folder @ .., MANIFEST_FILE] => (LinkTarget::Ref(Some(reference.to_string())), join(folder)),
            ["releases", "tag", tag] => (LinkTarget::Ref(Some(tag.to_string())), None),
            ["releases", "download", _, _] | ["releases", "latest", "download", _] | ["archive", ..] if is_zip => {
                (LinkTarget::File(rest.join("/")), None)
            }
            _ => {
                return Err(t("install.unsupportedLink"));
            }
        };
        let origin = format!("github.com/{owner}/{repo}{}", rest.iter().map(|s| format!("/{s}")).collect::<String>());
        Ok(Self { owner: owner.clone(), repo: repo.to_string(), target, origin, folder })
    }

    /// The repository, which is what the author controls: `github.com/<owner>/<repo>`, in lower
    /// case as GitHub compares names.
    fn source(&self) -> String {
        format!("github.com/{}/{}", self.owner, self.repo).to_ascii_lowercase()
    }

    fn archive_url(&self) -> String {
        let encode = |segment: &str| utf8_percent_encode(segment, PATH_SEGMENT).to_string();
        let base = format!("https://github.com/{}/{}", encode(&self.owner), encode(&self.repo));
        match &self.target {
            LinkTarget::Ref(reference) => {
                format!("{base}/archive/{}.zip", encode(reference.as_deref().unwrap_or("HEAD")))
            }
            LinkTarget::File(path) => format!("{base}/{}", path.split('/').map(encode).collect::<Vec<_>>().join("/")),
        }
    }
}

fn join(folder: &[&str]) -> Option<String> {
    (!folder.is_empty()).then(|| folder.join("/"))
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent = crate::http::agent(Duration::from_secs(180));
    log::info!("downloading a widget from {url}");
    let mut response = agent.get(url).call().map_err(|err| match err {
        ureq::Error::StatusCode(404) => t("install.notFound"),
        ureq::Error::StatusCode(code) => tf("install.httpError", &[("code", &code)]),
        err => tf("install.downloadFailed", &[("error", &err)]),
    })?;
    response.body_mut().with_config().limit(MAX_ARCHIVE_BYTES).read_to_vec().map_err(|err| match err {
        ureq::Error::BodyExceedsLimit(_) => archive_too_large(),
        err => tf("install.downloadFailed", &[("error", &err)]),
    })
}

fn is_zip(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

fn read_archive(path: &Path) -> Result<Vec<u8>, String> {
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_ARCHIVE_BYTES {
        return Err(archive_too_large());
    }
    fs::read(path).map_err(|e| e.to_string())
}

fn archive_too_large() -> String {
    tf("install.archiveTooLarge", &[("size", &(MAX_ARCHIVE_BYTES / 1024 / 1024))])
}

/// Counts what is written for one widget and stops at the limits.
#[derive(Default)]
struct Budget {
    files: usize,
    bytes: u64,
}

impl Budget {
    fn add_file(&mut self) -> Result<(), String> {
        self.files += 1;
        if self.files > MAX_WIDGET_FILES {
            return Err(tf("install.tooManyFiles", &[("count", &MAX_WIDGET_FILES)]));
        }
        Ok(())
    }

    /// Copies at most what is left of the size limit.
    fn copy(&mut self, from: &mut impl Read, to: &Path) -> Result<(), String> {
        let mut file = File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let left = MAX_WIDGET_BYTES - self.bytes;
        self.bytes += io::copy(&mut from.take(left + 1), &mut file).map_err(|e| e.to_string())?;
        if self.bytes > MAX_WIDGET_BYTES {
            return Err(tf("install.widgetTooLarge", &[("size", &(MAX_WIDGET_BYTES / 1024 / 1024))]));
        }
        Ok(())
    }
}

/// Unpacks the widget in the archive into `dest`: the folder with the shallowest
/// `manifest.json`, or `folder` for a GitHub folder link. Entries whose names would land outside
/// `dest`, and symbolic links, are skipped.
fn unpack(archive: &[u8], folder: Option<&str>, dest: &Path) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(archive)).map_err(|err| tf("install.notAZip", &[("error", &err)]))?;
    let names: Vec<Option<PathBuf>> = (0..archive.len())
        .map(|index| archive.by_index_raw(index).map(|entry| entry.enclosed_name()))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let root = widget_root(&names, folder)?;

    let mut budget = Budget::default();
    for (index, name) in names.iter().enumerate() {
        let Some(relative) = name.as_ref().and_then(|name| name.strip_prefix(&root).ok()) else { continue };
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let plain = relative.components().all(|c| matches!(c, Component::Normal(_)));
        if !plain || relative.as_os_str().is_empty() || entry.is_dir() || entry.is_symlink() {
            continue;
        }
        budget.add_file()?;
        let path = dest.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        budget.copy(&mut entry, &path)?;
    }
    Ok(())
}

/// Finds the widget folder among the archive's entry names.
fn widget_root(names: &[Option<PathBuf>], folder: Option<&str>) -> Result<PathBuf, String> {
    let roots: Vec<&Path> = names
        .iter()
        .flatten()
        .filter(|name| name.file_name().is_some_and(|file| file == MANIFEST_FILE))
        .map(|name| name.parent().unwrap_or(Path::new("")))
        .collect();

    if let Some(folder) = folder {
        // A GitHub archive keeps everything in one top folder named after the repository and ref.
        let wanted: Vec<&str> = folder.split('/').collect();
        return roots
            .into_iter()
            .find(|root| root.components().skip(1).map(|c| c.as_os_str().to_str()).eq(wanted.iter().map(|s| Some(*s))))
            .map(Path::to_path_buf)
            .ok_or_else(|| tf("install.noManifestInFolder", &[("folder", &folder)]));
    }

    let depth = roots
        .iter()
        .map(|root| root.components().count())
        .min()
        .ok_or_else(|| t("install.noManifest"))?;
    let shallowest: Vec<&Path> = roots.into_iter().filter(|root| root.components().count() == depth).collect();
    match shallowest.as_slice() {
        [root] => Ok(root.to_path_buf()),
        several => {
            let list: Vec<String> = several.iter().map(|root| root.display().to_string().replace('\\', "/")).collect();
            Err(tf("install.severalWidgets", &[("list", &list.join(", "))]))
        }
    }
}

/// Copies a widget folder, skipping `.git` and symbolic links.
fn copy_folder(from: &Path, to: &Path, budget: &mut Budget) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if kind.is_dir() && entry.file_name() != ".git" {
            copy_folder(&entry.path(), &target, budget)?;
        } else if kind.is_file() {
            budget.add_file()?;
            let mut file = File::open(entry.path()).map_err(|e| e.to_string())?;
            budget.copy(&mut file, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn link(url: &str) -> GitHubLink {
        GitHubLink::parse(url).unwrap_or_else(|err| panic!("{url}: {err}"))
    }

    #[test]
    fn parses_github_links() {
        let repo = link("https://github.com/someone/weather-widget");
        assert_eq!((repo.owner.as_str(), repo.repo.as_str()), ("someone", "weather-widget"));
        assert_eq!(repo.archive_url(), "https://github.com/someone/weather-widget/archive/HEAD.zip");
        assert_eq!(link("github.com/someone/weather-widget.git/").archive_url(), repo.archive_url());
        assert_eq!(link("www.github.com/someone/weather-widget?tab=readme#install").archive_url(), repo.archive_url());

        let folder = link("https://github.com/someone/widgets/tree/main/weather/v2");
        assert_eq!(folder.target, LinkTarget::Ref(Some("main".into())));
        assert_eq!(folder.folder.as_deref(), Some("weather/v2"));
        assert_eq!(folder.archive_url(), "https://github.com/someone/widgets/archive/main.zip");
        assert_eq!(folder.origin, "github.com/someone/widgets/tree/main/weather/v2");

        let blob = link("https://github.com/someone/widgets/blob/v1.0/my%20clock/manifest.json");
        assert_eq!(blob.folder.as_deref(), Some("my clock"));
        assert_eq!(blob.archive_url(), "https://github.com/someone/widgets/archive/v1.0.zip");

        let release = link("https://github.com/someone/clock/releases/tag/v1.2.0");
        assert_eq!(release.archive_url(), "https://github.com/someone/clock/archive/v1.2.0.zip");

        assert_eq!(link("https://github.com/SomeOne/Widgets/tree/main/clock").source(), "github.com/someone/widgets");

        let asset = link("https://github.com/someone/clock/releases/download/v1.2.0/clock.zip");
        assert_eq!(asset.archive_url(), "https://github.com/someone/clock/releases/download/v1.2.0/clock.zip");
        assert!(GitHubLink::parse("https://github.com/someone/clock/releases/latest/download/clock.zip").is_ok());
    }

    #[test]
    fn rejects_links_that_are_not_widgets_on_github() {
        for bad in [
            "",
            "https://gitlab.com/someone/clock",
            "https://github.com/someone",
            "https://github.com/someone/clock/issues/1",
            "https://github.com/someone/clock/releases/download/v1/clock.exe",
            "https://github.com/some one/clock",
            "https://github.com.evil.example/someone/clock",
        ] {
            assert!(GitHubLink::parse(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn notices_a_widget_from_another_source() {
        let repo = "github.com/someone/clock";
        assert!(!source_changed(Some(repo), repo, true));
        assert!(source_changed(Some(repo), "github.com/other/clock", true));
        assert!(source_changed(Some(repo), LOCAL_SOURCE, false));
        // Nothing recorded: a change only if the id was installed or has settings.
        assert!(source_changed(None, repo, true));
        assert!(!source_changed(None, repo, false));
    }

    fn names(list: &[&str]) -> Vec<Option<PathBuf>> {
        list.iter().map(|name| Some(PathBuf::from(name))).collect()
    }

    #[test]
    fn finds_the_widget_in_an_archive() {
        let repo = names(&["clock-main/", "clock-main/manifest.json", "clock-main/examples/x/manifest.json"]);
        assert_eq!(widget_root(&repo, None).unwrap(), PathBuf::from("clock-main"));

        let flat = names(&["manifest.json", "index.html"]);
        assert_eq!(widget_root(&flat, None).unwrap(), PathBuf::from(""));

        let several = names(&["w-main/a/manifest.json", "w-main/b/manifest.json"]);
        assert!(widget_root(&several, None).unwrap_err().contains("several widgets"));
        assert_eq!(widget_root(&several, Some("b")).unwrap(), PathBuf::from("w-main/b"));
        assert!(widget_root(&several, Some("c")).is_err());
        assert!(widget_root(&names(&["readme.md"]), None).is_err());
    }

    fn zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer.start_file(*name, SimpleFileOptions::default()).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lizun-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn unpacks_only_the_widget_folder() {
        let archive = zip(&[
            ("repo-main/README.md", "readme"),
            ("repo-main/widget/manifest.json", "{}"),
            ("repo-main/widget/js/main.js", "code"),
            ("repo-main/widget/../../escape.txt", "nope"),
            ("../../outside.txt", "nope"),
        ]);
        let dest = temp_dir("unpack");
        unpack(&archive, None, &dest).unwrap();
        assert!(dest.join("manifest.json").is_file());
        assert_eq!(fs::read_to_string(dest.join("js/main.js")).unwrap(), "code");
        assert!(!dest.join("README.md").exists());
        assert!(!dest.parent().unwrap().join("escape.txt").exists());
        fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn stages_and_validates_a_local_widget() {
        let hello = crate::widgets::repo_path("widgets/hello");
        let dir = temp_dir("stage-folder");
        let source = InstallSource::Path { path: hello.clone() };
        let (manifest, _, _) = stage(&source, &dir).unwrap();
        assert_eq!(manifest.id, "dev.lizun.hello");
        assert!(dir.join("widget/main.js").is_file());
        fs::remove_dir_all(&dir).unwrap();

        // The same widget zipped with its folder, as people usually zip things.
        let files: Vec<(String, String)> = fs::read_dir(&hello)
            .unwrap()
            .flatten()
            .map(|e| (format!("hello/{}", e.file_name().to_string_lossy()), fs::read_to_string(e.path()).unwrap()))
            .collect();
        let entries: Vec<(&str, &str)> = files.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect();
        let archive_path = temp_dir("hello").with_extension("zip");
        fs::write(&archive_path, zip(&entries)).unwrap();
        let dir = temp_dir("stage-zip");
        let (manifest, _, _) = stage(&InstallSource::Path { path: archive_path.clone() }, &dir).unwrap();
        assert_eq!(manifest.id, "dev.lizun.hello");
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_file(&archive_path).unwrap();
    }

    #[test]
    fn refuses_an_invalid_widget() {
        let archive_path = temp_dir("broken").with_extension("zip");
        fs::write(&archive_path, zip(&[("manifest.json", r#"{"id":"Bad Id","name":"x","version":"1"}"#)])).unwrap();
        let dir = temp_dir("stage-broken");
        let err = stage(&InstallSource::Path { path: archive_path.clone() }, &dir).unwrap_err();
        assert!(err.contains("invalid id"), "{err}");

        fs::write(&archive_path, zip(&[("manifest.json", r#"{"id":"com.example.x","name":"x","version":"1"}"#)])).unwrap();
        let err = stage(&InstallSource::Path { path: archive_path.clone() }, &dir).unwrap_err();
        assert!(err.contains("index.html"), "{err}");
        let _ = fs::remove_dir_all(&dir);
        fs::remove_file(&archive_path).unwrap();
    }

    #[test]
    fn replaces_the_installed_copy_and_rolls_back_on_failure() {
        let root = temp_dir("replace");
        let widgets = root.join("widgets");
        let old = widgets.join("old-folder-name");
        let target = widgets.join("com.example.clock");
        let staging = root.join("staging/1");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("version"), "1").unwrap();
        fs::create_dir_all(staging.join("widget")).unwrap();
        fs::write(staging.join("widget/version"), "2").unwrap();

        // The new copy cannot be moved: nothing is lost.
        let missing = root.join("staging/2");
        fs::create_dir_all(&missing).unwrap();
        assert!(move_into_place(&missing, &target, std::slice::from_ref(&old)).is_err());
        assert_eq!(fs::read_to_string(old.join("version")).unwrap(), "1");

        move_into_place(&staging, &target, std::slice::from_ref(&old)).unwrap();
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "2");
        assert!(!old.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    /// Downloads a real repository, which is not a widget; run with `cargo test -- --ignored`
    /// when online.
    #[test]
    #[ignore]
    fn downloads_from_github() {
        let dir = temp_dir("stage-github");
        let source = InstallSource::Github { url: "https://github.com/octocat/Hello-World".into() };
        let err = stage(&source, &dir).unwrap_err();
        assert!(err.contains("no manifest.json"), "{err}");
        let source = InstallSource::Github { url: "https://github.com/octocat/no-such-repository-here".into() };
        assert!(stage(&source, &dir).unwrap_err().contains("Nothing found"));
        let _ = fs::remove_dir_all(&dir);
    }
}
