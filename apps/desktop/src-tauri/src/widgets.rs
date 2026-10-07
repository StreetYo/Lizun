//! Third-party widgets: discovery, manifest validation and the `lizun-widget` protocol that
//! serves widget files to sandboxed iframes in the island.
//!
//! A widget is a folder with a `manifest.json` and web files. It runs in an iframe with an
//! opaque origin, so it cannot reach the island page or the Tauri IPC; it talks to the island
//! only through the SDK (postMessage), and the island checks every call against the
//! permissions declared in the manifest. Network access is limited by a CSP built from the
//! manifest's `network` list.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Emitter, Manager, Runtime, UriSchemeContext};

pub const SCHEME: &str = "lizun-widget";
/// Sent to every window after the widget list was reloaded.
pub const EVENT_CHANGED: &str = "widgets://changed";
/// Path prefix for files shared by all widgets: the SDK module and the base stylesheet.
const SDK_PREFIX: &str = "__sdk";
const MANIFEST_FILE: &str = "manifest.json";

/// Permissions a widget can request. The island refuses SDK calls that need anything else.
pub const KNOWN_PERMISSIONS: &[&str] = &["media:read", "media:control", "storage", "activity"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WidgetManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    /// Width of the widget's column in the expanded island, in logical pixels.
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Origins the widget may fetch from, such as `https://api.example.com`.
    #[serde(default)]
    pub network: Vec<String>,
    /// Options the user changes in the settings window.
    #[serde(default)]
    pub settings: Vec<crate::widget_settings::SettingSpec>,
}

fn default_entry() -> String {
    "index.html".into()
}

fn default_width() -> u32 {
    240
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledWidget {
    #[serde(flatten)]
    pub manifest: WidgetManifest,
    /// `bundled` for widgets shipped with Lizun, `dev` for the repository's examples (debug
    /// builds only), `user` for widgets the user installed.
    pub source: &'static str,
    #[serde(skip)]
    pub dir: PathBuf,
}

#[derive(Default)]
struct Registry {
    widgets: RwLock<Vec<InstalledWidget>>,
}

pub fn init(app: &AppHandle) {
    app.manage(Registry::default());
    refresh(app);
}

#[tauri::command]
pub fn widgets_list(app: AppHandle) -> Vec<InstalledWidget> {
    refresh(&app)
}

/// Rescans the widget folders and tells the island to pick up the changes.
#[tauri::command]
pub fn widgets_reload(app: AppHandle) -> Vec<InstalledWidget> {
    let widgets = refresh(&app);
    if let Err(err) = app.emit(EVENT_CHANGED, ()) {
        log::warn!("failed to announce the widget reload: {err}");
    }
    widgets
}

/// Opens the folder where users put their widgets.
#[tauri::command]
pub fn widgets_open_folder(app: AppHandle) -> Result<(), String> {
    let dir = user_widgets_dir(&app).ok_or("no widgets folder")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::platform::reveal_folder(&dir).map_err(|e| e.to_string())
}

/// Manifests of the widgets found by the last scan.
pub fn installed<R: Runtime>(app: &AppHandle<R>) -> Vec<WidgetManifest> {
    let registry = app.state::<Registry>();
    let widgets = registry.widgets.read().unwrap();
    widgets.iter().map(|w| w.manifest.clone()).collect()
}

pub fn find<R: Runtime>(app: &AppHandle<R>, id: &str) -> Option<WidgetManifest> {
    let registry = app.state::<Registry>();
    let widgets = registry.widgets.read().unwrap();
    widgets.iter().find(|w| w.manifest.id == id).map(|w| w.manifest.clone())
}

pub fn user_widgets_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join("widgets"))
}

fn refresh<R: Runtime>(app: &AppHandle<R>) -> Vec<InstalledWidget> {
    let mut found: Vec<InstalledWidget> = Vec::new();
    for (root, source) in widget_roots(app) {
        let Ok(entries) = fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.join(MANIFEST_FILE).is_file() {
                continue;
            }
            match load_manifest(&dir) {
                Ok(manifest) if found.iter().any(|w| w.manifest.id == manifest.id) => {
                    log::warn!("skipping {}: widget id {} is already installed", dir.display(), manifest.id);
                }
                Ok(manifest) => found.push(InstalledWidget { manifest, source, dir }),
                Err(err) => log::warn!("skipping widget in {}: {err}", dir.display()),
            }
        }
    }
    *app.state::<Registry>().widgets.write().unwrap() = found.clone();
    found
}

/// Folders to scan, in priority order: a widget id found in an earlier folder cannot be taken
/// by a widget in a later one, so an installed widget cannot replace a bundled one.
fn widget_roots<R: Runtime>(app: &AppHandle<R>) -> Vec<(PathBuf, &'static str)> {
    let mut roots = Vec::new();
    match bundled_dir(app) {
        Some(bundled) => roots.push((bundled, "bundled")),
        None => log::warn!("no resource directory for the bundled widgets"),
    }
    if cfg!(debug_assertions) {
        roots.push((repo_path("widgets"), "dev"));
    }
    match user_widgets_dir(app) {
        Some(user) => {
            if let Err(err) = fs::create_dir_all(&user) {
                log::warn!("cannot create {}: {err}", user.display());
            }
            roots.push((user, "user"));
        }
        None => log::warn!("no app data directory"),
    }
    roots
}

fn bundled_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return Some(repo_path("apps/desktop/bundled-widgets"));
    }
    app.path().resource_dir().ok().map(|dir| dir.join("bundled-widgets"))
}

fn sdk_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return Some(repo_path("packages/widget-sdk/dist"));
    }
    app.path().resource_dir().ok().map(|dir| dir.join("widget-sdk"))
}

/// Path inside the source repository; only meaningful for debug builds run from a checkout.
fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").join(relative)
}

fn load_manifest(dir: &Path) -> Result<WidgetManifest, String> {
    let text = fs::read_to_string(dir.join(MANIFEST_FILE)).map_err(|e| e.to_string())?;
    let manifest: WidgetManifest = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    validate(&manifest)?;
    Ok(manifest)
}

fn validate(manifest: &WidgetManifest) -> Result<(), String> {
    let id = &manifest.id;
    let id_chars_ok = id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    if !(3..=64).contains(&id.len()) || !id_chars_ok || !id.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return Err(format!("invalid id {id:?}: use 3-64 lowercase letters, digits, dots or dashes"));
    }
    if safe_relative_path(&manifest.entry).is_none() {
        return Err(format!("invalid entry {:?}", manifest.entry));
    }
    if !(120..=640).contains(&manifest.width) {
        return Err(format!("width {} is outside 120..=640", manifest.width));
    }
    if let Some(unknown) = manifest.permissions.iter().find(|p| !KNOWN_PERMISSIONS.contains(&p.as_str())) {
        return Err(format!("unknown permission {unknown:?}"));
    }
    if let Some(bad) = manifest.network.iter().find(|origin| !is_valid_origin(origin)) {
        return Err(format!("invalid network origin {bad:?}: expected https://host[:port]"));
    }
    crate::widget_settings::validate(&manifest.settings)
}

/// Accepts `https://host[:port]` with an optional leading `*.` wildcard, nothing else, so a
/// manifest cannot smuggle extra CSP directives.
fn is_valid_origin(origin: &str) -> bool {
    let Some(host) = origin.strip_prefix("https://") else { return false };
    let host = host.strip_prefix("*.").unwrap_or(host);
    let (name, port) = host.split_once(':').unwrap_or((host, ""));
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && port.chars().all(|c| c.is_ascii_digit())
}

/// Returns the path only if it is relative and made of plain components (no `..`, roots,
/// drive prefixes or backslashes), so it cannot escape the widget folder.
fn safe_relative_path(path: &str) -> Option<&Path> {
    let candidate = Path::new(path);
    let plain = !path.is_empty()
        && !path.contains(['\\', ':'])
        && candidate.components().all(|c| matches!(c, Component::Normal(_)));
    plain.then_some(candidate)
}

pub fn serve<R: Runtime>(ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match resolve(ctx.app_handle(), request.uri().path()) {
        Ok(file) => file.into_response(),
        Err(status) => Response::builder()
            .status(status)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Vec::new())
            .unwrap(),
    }
}

struct ServedFile {
    path: PathBuf,
    bytes: Vec<u8>,
    csp: Option<String>,
}

impl ServedFile {
    fn into_response(self) -> Response<Vec<u8>> {
        let mut response = Response::builder()
            .header(header::CONTENT_TYPE, mime_type(&self.path))
            // Widget iframes have an opaque origin, and module scripts are fetched with CORS.
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Content-Type-Options", "nosniff");
        if let Some(csp) = self.csp {
            response = response.header(header::CONTENT_SECURITY_POLICY, csp);
        }
        response.body(self.bytes).unwrap()
    }
}

fn resolve<R: Runtime>(app: &AppHandle<R>, raw_path: &str) -> Result<ServedFile, StatusCode> {
    let path = percent_decode_str(raw_path).decode_utf8().map_err(|_| StatusCode::BAD_REQUEST)?;
    let (first, rest) = path.trim_start_matches('/').split_once('/').unwrap_or((path.trim_start_matches('/'), ""));

    let (base, relative, csp) = if first == SDK_PREFIX {
        (sdk_dir(app).ok_or(StatusCode::NOT_FOUND)?, rest.to_string(), None)
    } else {
        let widgets = app.state::<Registry>();
        let widgets = widgets.widgets.read().unwrap();
        let widget = widgets.iter().find(|w| w.manifest.id == first).ok_or(StatusCode::NOT_FOUND)?;
        let relative = if rest.is_empty() { widget.manifest.entry.clone() } else { rest.to_string() };
        (widget.dir.clone(), relative, Some(widget_csp(&widget.manifest)))
    };

    let relative = safe_relative_path(&relative).ok_or(StatusCode::FORBIDDEN)?;
    let base = base.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let path = base.join(relative).canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    if !path.starts_with(&base) || !path.is_file() {
        return Err(StatusCode::FORBIDDEN);
    }
    let bytes = fs::read(&path).map_err(|_| StatusCode::NOT_FOUND)?;
    let csp = csp.filter(|_| mime_type(&path).starts_with("text/html"));
    Ok(ServedFile { path, bytes, csp })
}

fn widget_csp(manifest: &WidgetManifest) -> String {
    let origin = if cfg!(windows) {
        format!("http://{SCHEME}.localhost")
    } else {
        format!("{SCHEME}://localhost")
    };
    let own = format!("{origin}/{}/", manifest.id);
    let sdk = format!("{origin}/{SDK_PREFIX}/");
    let connect = if manifest.network.is_empty() {
        "'none'".to_string()
    } else {
        manifest.network.join(" ")
    };
    format!(
        "default-src 'none'; script-src {own} {sdk}; style-src {own} {sdk} 'unsafe-inline'; \
         img-src {own} data: blob:; font-src {own} {sdk}; media-src {own} data: blob:; \
         connect-src {connect}; form-action 'none'; base-uri 'none'"
    )
}

fn mime_type(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str) -> WidgetManifest {
        WidgetManifest {
            id: id.into(),
            name: "Test".into(),
            version: "1.0.0".into(),
            author: String::new(),
            description: String::new(),
            entry: default_entry(),
            width: default_width(),
            permissions: vec!["media:read".into()],
            network: vec![],
            settings: vec![],
        }
    }

    #[test]
    fn rejects_paths_that_escape_the_widget_folder() {
        for bad in ["../secret.txt", "a/../../b", "/etc/passwd", "C:\\Windows\\win.ini", "c:/x", "a\\b", ""] {
            assert!(safe_relative_path(bad).is_none(), "{bad} should be rejected");
        }
        assert!(safe_relative_path("assets/icon.png").is_some());
    }

    #[test]
    fn validates_ids_permissions_and_origins() {
        assert!(validate(&manifest("dev.lizun.hello")).is_ok());
        assert!(validate(&manifest("Bad Id")).is_err());
        assert!(validate(&manifest("__sdk")).is_err());

        let mut unknown = manifest("dev.lizun.hello");
        unknown.permissions.push("files:write".into());
        assert!(validate(&unknown).is_err());

        let mut network = manifest("dev.lizun.hello");
        network.network = vec!["https://api.example.com".into(), "https://*.example.org:8443".into()];
        assert!(validate(&network).is_ok());
        for bad in ["http://api.example.com", "https://a.com/path", "https://a.com; script-src *", "*"] {
            network.network = vec![bad.into()];
            assert!(validate(&network).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn example_and_bundled_manifests_are_valid() {
        let manifest = load_manifest(&repo_path("widgets/hello")).unwrap();
        assert_eq!(manifest.settings.len(), 4);
        let bundled = fs::read_dir(repo_path("apps/desktop/bundled-widgets")).unwrap();
        for dir in bundled.flatten() {
            load_manifest(&dir.path()).unwrap_or_else(|err| panic!("{}: {err}", dir.path().display()));
        }
    }

    #[test]
    fn csp_limits_scripts_and_network_to_the_manifest() {
        let mut m = manifest("dev.lizun.hello");
        m.network = vec!["https://api.example.com".into()];
        let csp = widget_csp(&m);
        assert!(csp.contains("default-src 'none'"));
        assert!(csp.contains("connect-src https://api.example.com;"));
        assert!(csp.contains("/dev.lizun.hello/"));
    }
}
