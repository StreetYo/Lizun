//! Network requests a widget makes through the core (`network.request` in the SDK).
//!
//! A widget's own `fetch` is subject to CORS, and its iframe sends `Origin: null`, so it cannot
//! talk to APIs that do not allow every origin, such as local apps (AnkiConnect on
//! `127.0.0.1:8765`). The core sends the request instead, without an `Origin` header, but only
//! to the origins in the widget's `network` list and without following redirects, so the list
//! stays the limit. Why it works this way is in docs/adr/0010-widget-network-and-storage.md.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Url};

/// How long a request may take, connecting included.
const TIMEOUT: Duration = Duration::from_secs(60);
/// Larger request bodies are refused.
const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
/// Larger responses are refused.
const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
const METHODS: &[&str] = &["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"];
/// Requests one widget may have in flight at once; each answer can hold up to 32 MB.
const MAX_IN_FLIGHT: usize = 4;
/// Headers the core sets itself.
const RESERVED_HEADERS: &[&str] = &["host", "content-length", "connection", "transfer-encoding", "origin"];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRequest {
    url: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    /// Text sent as the request body.
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WidgetResponse {
    status: u16,
    /// By lowercase name; repeated headers are joined with `, `.
    headers: BTreeMap<String, String>,
    /// The body as text; bytes that are not UTF-8 are replaced.
    body: String,
}

/// Requests in flight by widget id.
static IN_FLIGHT: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

/// Counts a widget's request while it is in flight.
struct Slot(String);

impl Slot {
    fn take(widget_id: &str) -> Result<Self, String> {
        let mut in_flight = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        let count = in_flight.get_or_insert_with(HashMap::new).entry(widget_id.to_string()).or_default();
        if *count >= MAX_IN_FLIGHT {
            return Err(format!("more than {MAX_IN_FLIGHT} requests at once"));
        }
        *count += 1;
        Ok(Self(widget_id.to_string()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut in_flight = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = in_flight.as_mut().and_then(|map| map.get_mut(&self.0)) {
            *count = count.saturating_sub(1);
        }
    }
}

#[tauri::command]
pub async fn widget_network_request(
    app: AppHandle,
    widget_id: String,
    request: WidgetRequest,
) -> Result<WidgetResponse, String> {
    let manifest = crate::widgets::find(&app, &widget_id).ok_or("unknown widget")?;
    // Addresses the user typed into the widget's `url` settings count as listed.
    let mut network = manifest.network.clone();
    network.extend(crate::widget_settings::user_origins(&app, &manifest));
    // Lizun's own browser extension server, wherever the user moved it.
    let lizun_port = crate::preferences::current(&app).browser_media_port;
    let slot = Slot::take(&widget_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _slot = slot;
        send(&network, request, lizun_port)
    })
    .await
    .map_err(|err| err.to_string())?
}

fn send(network: &[String], request: WidgetRequest, lizun_port: u16) -> Result<WidgetResponse, String> {
    let url = Url::parse(&request.url).map_err(|err| format!("invalid url: {err}"))?;
    if !allowed(network, &url) {
        return Err(format!("{} is not in the network list of manifest.json or the widget's address settings", origin_of(&url)));
    }
    let port = url.port_or_known_default().unwrap_or(0);
    if url.scheme() == "http" && (port == lizun_port || crate::widgets::FORBIDDEN_LOOPBACK_PORTS.contains(&port)) {
        return Err(format!("port {port} on this computer cannot be reached by widgets"));
    }
    let method = request.method.as_deref().unwrap_or("GET").to_ascii_uppercase();
    if !METHODS.contains(&method.as_str()) {
        return Err(format!("unsupported method {method}"));
    }
    if request.body.as_ref().is_some_and(|body| body.len() > MAX_REQUEST_BYTES) {
        return Err(format!("the body exceeds {} MB", MAX_REQUEST_BYTES / 1024 / 1024));
    }

    let mut builder = ureq::http::Request::builder().method(method.as_str()).uri(url.as_str());
    for (name, value) in &request.headers {
        if RESERVED_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            return Err(format!("the {name} header cannot be set"));
        }
        builder = builder.header(name, value);
    }
    let agent: ureq::Agent = crate::http::config(TIMEOUT)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .allow_non_standard_methods(false)
        .build()
        .into();
    let result = match request.body {
        Some(body) => builder.body(body).map_err(|err| err.to_string()).map(|r| agent.run(r)),
        None => builder.body(()).map_err(|err| err.to_string()).map(|r| agent.run(r)),
    }?;
    let mut response = result.map_err(|err| err.to_string())?;

    let mut headers = BTreeMap::<String, String>::new();
    for (name, value) in response.headers() {
        let Ok(value) = value.to_str() else { continue };
        headers
            .entry(name.as_str().to_string())
            .and_modify(|joined| {
                joined.push_str(", ");
                joined.push_str(value);
            })
            .or_insert_with(|| value.to_string());
    }
    let bytes = response.body_mut().with_config().limit(MAX_RESPONSE_BYTES).read_to_vec().map_err(|err| match err {
        ureq::Error::BodyExceedsLimit(_) => format!("the response exceeds {} MB", MAX_RESPONSE_BYTES / 1024 / 1024),
        err => err.to_string(),
    })?;
    Ok(WidgetResponse {
        status: response.status().as_u16(),
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

fn origin_of(url: &Url) -> String {
    url.origin().ascii_serialization()
}

/// Whether `url` is on one of the origins of a `network` list. Like in a CSP, an origin
/// without a port means the scheme's default port, and `*.example.org` matches its subdomains
/// but not `example.org` itself.
fn allowed(network: &[String], url: &Url) -> bool {
    let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) else { return false };
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    network.iter().any(|origin| matches(origin, url.scheme(), host, port))
}

fn matches(origin: &str, scheme: &str, host: &str, port: u16) -> bool {
    let Some((origin_scheme, rest)) = origin.split_once("://") else { return false };
    if origin_scheme != scheme {
        return false;
    }
    // The last colon starts the port, unless it is inside an IPv6 address such as `[fd00::1]`.
    let default_port = if scheme == "https" { 443 } else { 80 };
    let (pattern, origin_port) = match rest.rsplit_once(':') {
        Some((pattern, port)) if !port.contains(']') => (pattern, port.parse::<u16>().ok()),
        _ => (rest, Some(default_port)),
    };
    if origin_port != Some(port) {
        return false;
    }
    let pattern = pattern.to_ascii_lowercase();
    match pattern.strip_prefix("*.") {
        Some(parent) => host.strip_suffix(parent).is_some_and(|sub| sub.len() > 1 && sub.ends_with('.')),
        None => host == pattern,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(network: &[&str], url: &str) -> bool {
        let network: Vec<String> = network.iter().map(|origin| origin.to_string()).collect();
        allowed(&network, &Url::parse(url).unwrap())
    }

    #[test]
    fn allows_only_the_listed_origins() {
        assert!(check(&["http://127.0.0.1:8765"], "http://127.0.0.1:8765"));
        assert!(check(&["http://127.0.0.1:8765"], "http://127.0.0.1:8765/path?x=1"));
        assert!(!check(&["http://127.0.0.1:8765"], "http://127.0.0.1:8766"));
        assert!(!check(&["http://127.0.0.1:8765"], "http://localhost:8765"));
        assert!(!check(&["http://127.0.0.1:8765"], "https://127.0.0.1:8765"));
        assert!(!check(&["http://127.0.0.1:8765"], "http://user@127.0.0.1:8765"));

        assert!(check(&["https://api.example.com"], "https://api.example.com/v1"));
        assert!(check(&["https://api.example.com"], "https://API.example.com:443/v1"));
        assert!(!check(&["https://api.example.com"], "https://api.example.com:8443"));
        assert!(!check(&["https://api.example.com"], "https://api.example.com.evil.net"));
        assert!(!check(&["https://api.example.com"], "http://api.example.com"));

        assert!(check(&["https://*.example.org"], "https://a.b.example.org"));
        assert!(!check(&["https://*.example.org"], "https://example.org"));
        assert!(!check(&["https://*.example.org"], "https://badexample.org"));
        assert!(!check(&[], "https://example.org"));

        // Origins from the user's address settings: local servers and IPv6 hosts.
        assert!(check(&["http://192.168.1.5:8123"], "http://192.168.1.5:8123/api/states"));
        assert!(check(&["http://[fd00::1]:8123"], "http://[fd00::1]:8123/"));
        assert!(check(&["http://[fd00::1]"], "http://[fd00::1]/"));
        assert!(!check(&["http://[fd00::1]"], "http://[fd00::1]:8080/"));
    }

    #[test]
    fn refuses_requests_off_the_list_before_sending() {
        let request = |url: &str, method: &str| WidgetRequest {
            url: url.into(),
            method: Some(method.into()),
            headers: BTreeMap::new(),
            body: None,
        };
        let network = vec!["http://127.0.0.1:9".to_string(), "http://127.0.0.1:7000".to_string()];
        let send = |request| send(&network, request, 7000);
        assert!(send(request("http://example.com", "GET")).unwrap_err().contains("not in the network list"));
        assert!(send(request("http://127.0.0.1:9", "CONNECT")).unwrap_err().contains("unsupported method"));
        let mut origin = request("http://127.0.0.1:9", "GET");
        origin.headers.insert("Origin".into(), "https://example.com".into());
        assert!(send(origin).unwrap_err().contains("cannot be set"));
        // The port Lizun's browser extension server was moved to.
        assert!(send(request("http://127.0.0.1:7000", "GET")).unwrap_err().contains("cannot be reached"));
    }

    #[test]
    fn limits_requests_in_flight() {
        let slots: Vec<Slot> = (0..MAX_IN_FLIGHT).map(|_| Slot::take("dev.test").unwrap()).collect();
        assert!(Slot::take("dev.test").is_err());
        assert!(Slot::take("dev.other").is_ok());
        drop(slots);
        assert!(Slot::take("dev.test").is_ok());
    }
}
