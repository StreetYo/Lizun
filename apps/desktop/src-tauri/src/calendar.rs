//! Events for the built-in calendar widget, read from a calendar feed: the iCal (.ics) link or
//! the path to an .ics file the user puts in the widget's settings (see ADR 0006).
//!
//! A background thread reads the feed when the setting changes and then every few minutes (a
//! file as soon as it changes), expands the events of the weeks around today and sends them to
//! the island. A link is a secret that gives read access to the calendar, so it is never logged.

use std::fs;
use std::io::ErrorKind;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, SystemTime};

use percent_encoding::percent_decode_str;

use chrono::{Days, NaiveTime, TimeZone, Utc};
use rrule::Tz;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::i18n::{t, tf};
use crate::ics::{self, Event};
use crate::widget_settings::{FileChoice, SelectOption, SettingSpec};

pub const WIDGET_ID: &str = "builtin:calendar";
/// Sent to every window with a [`Snapshot`] whenever the events or the state of the feed change.
pub const EVENT_CHANGED: &str = "calendar://changed";
const URL_KEY: &str = "icsUrl";
const REFRESH_KEY: &str = "refreshMinutes";
/// Feeds of calendars with years of history run to a few megabytes.
const MAX_FEED_BYTES: u64 = 20 * 1024 * 1024;
/// Days around today whose events are kept, enough for the week shown and the hours until the
/// next download.
const DAYS_BEFORE: u64 = 7;
const DAYS_AFTER: u64 = 21;
/// After a failed download, the next try comes sooner than the usual refresh.
const RETRY_AFTER_ERROR: Duration = Duration::from_secs(2 * 60);
/// How often a calendar file is checked for changes.
const FILE_POLL: Duration = Duration::from_secs(10);

/// The widget's options, shown under it in Settings > Widgets, in the current language.
pub fn settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::Text {
            key: URL_KEY.into(),
            label: t("calendar.settings.source"),
            description: t("calendar.settings.sourceDescription"),
            default: String::new(),
            max_length: 500,
            placeholder: t("calendar.settings.sourcePlaceholder"),
            file: Some(FileChoice { name: t("calendar.settings.fileType"), extensions: vec!["ics".into()] }),
        },
        SettingSpec::Number {
            key: REFRESH_KEY.into(),
            label: t("calendar.settings.refresh"),
            description: t("calendar.settings.refreshDescription"),
            default: 15.0,
            min: 5.0,
            max: 240.0,
            step: 5.0,
            unit: t("calendar.settings.refreshUnit"),
        },
        // Read by the island; the core does not need it.
        SettingSpec::Select {
            key: "endedToday".into(),
            label: t("calendar.settings.endedToday"),
            description: t("calendar.settings.endedTodayDescription"),
            default: "dim".into(),
            options: vec![
                SelectOption { value: "dim".into(), label: t("calendar.settings.showDimmed") },
                SelectOption { value: "hide".into(), label: t("calendar.settings.hide") },
            ],
        },
    ]
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// False while no link is set.
    pub configured: bool,
    /// True while the feed is being downloaded.
    pub loading: bool,
    /// Events of the weeks around today. Kept from the last download when a later one fails.
    pub events: Vec<Event>,
    pub error: Option<String>,
    /// When the feed was last read, in milliseconds since the Unix epoch.
    pub updated_at: Option<i64>,
    /// IANA name of the system time zone the days are counted in; empty when Windows uses a zone
    /// without one, and the island then goes by its own clock. The island's own idea of local
    /// time can lag behind a change in the system settings, so it formats with this one.
    pub time_zone: String,
    #[serde(skip)]
    refresh: Duration,
}

pub struct Calendar {
    snapshot: Mutex<Snapshot>,
    wake: Sender<()>,
}

pub fn start(app: &AppHandle) {
    let (wake, wakes) = mpsc::channel();
    app.manage(Calendar { snapshot: Mutex::default(), wake });
    let handle = app.clone();
    if let Err(err) = thread::Builder::new().name("lizun-calendar".into()).spawn(move || run(&handle, wakes)) {
        log::error!("failed to start the calendar thread: {err}");
    }
}

/// Called after the widget's settings change, so a new link is read at once.
pub fn settings_changed(app: &AppHandle) {
    if let Some(calendar) = app.try_state::<Calendar>() {
        let _ = calendar.wake.send(());
    }
}

/// The events known now. Asking for them also refreshes a feed that is older than the refresh
/// interval, which happens after the computer slept through the thread's timer.
#[tauri::command]
pub fn calendar_snapshot(calendar: State<'_, Calendar>) -> Snapshot {
    let mut snapshot = calendar.snapshot.lock().unwrap().clone();
    let age = snapshot.updated_at.map(|at| Utc::now().timestamp_millis() - at);
    let stale = age.is_none_or(|age| age > snapshot.refresh.as_millis() as i64);
    // After a change of the system time zone, all-day events move to the new midnights.
    let (_, time_zone) = local_zone();
    if snapshot.configured && !snapshot.loading && (stale || snapshot.time_zone != time_zone) {
        let _ = calendar.wake.send(());
    }
    if !snapshot.configured {
        snapshot.time_zone = time_zone;
    }
    snapshot
}

fn run(app: &AppHandle, wakes: Receiver<()>) {
    let mut last_input = String::new();
    let mut last_stamp = None;
    loop {
        let values = crate::widget_settings::values_of(app, WIDGET_ID);
        let input = values.get(URL_KEY).and_then(Value::as_str).unwrap_or_default().trim().to_string();
        let minutes = values.get(REFRESH_KEY).and_then(Value::as_f64).unwrap_or(15.0);
        let refresh = Duration::from_secs_f64(minutes.max(1.0) * 60.0);

        let wait = if input.is_empty() {
            publish(app, Snapshot { refresh, time_zone: local_zone().1, ..Snapshot::default() });
            None
        } else {
            Some(update(app, &input, input == last_input, refresh, &mut last_stamp))
        };
        last_input = input;

        let woken = match wait {
            Some(wait) => !matches!(wakes.recv_timeout(wait), Err(RecvTimeoutError::Disconnected)),
            None => wakes.recv().is_ok(),
        };
        if !woken {
            return;
        }
        // Several changes in a row need one download.
        while wakes.try_recv().is_ok() {}
    }
}

/// Reads the feed and publishes the result; returns how long to wait before the next read.
/// `stamp` keeps the size and modification time of a calendar file between calls.
fn update(app: &AppHandle, input: &str, same_feed: bool, refresh: Duration, stamp: &mut Option<FileStamp>) -> Duration {
    let source = Source::parse(input);
    let (zone, time_zone) = local_zone();
    let previous = app.state::<Calendar>().snapshot.lock().unwrap().clone();
    let is_file = matches!(source, Ok(Source::File(_)));
    if let Ok(Source::File(path)) = &source {
        // A file that has not changed is read again only when the window of days moves on.
        let current = file_stamp(path);
        let age = previous.updated_at.map(|at| Utc::now().timestamp_millis() - at);
        let fresh = previous.error.is_none()
            && previous.time_zone == time_zone
            && age.is_some_and(|age| age < refresh.as_millis() as i64);
        if same_feed && fresh && current.is_some() && current == *stamp {
            return FILE_POLL;
        }
        *stamp = current;
    }

    let mut snapshot = if same_feed { previous } else { Snapshot::default() };
    snapshot.configured = true;
    snapshot.refresh = refresh;
    snapshot.time_zone = time_zone;
    snapshot.loading = true;
    // The island shows a spinner only for a new link; marking the refresh keeps
    // `calendar_snapshot` from asking for a second one meanwhile.
    if same_feed {
        app.state::<Calendar>().snapshot.lock().unwrap().loading = true;
    } else {
        publish(app, snapshot.clone());
    }
    snapshot.loading = false;
    let wait = match source.and_then(|source| source.read()) {
        Ok(text) => {
            snapshot.events = events_around_today(&text, zone);
            snapshot.error = None;
            snapshot.updated_at = Some(Utc::now().timestamp_millis());
            refresh
        }
        Err(err) => {
            snapshot.error = Some(err);
            refresh.min(RETRY_AFTER_ERROR)
        }
    };
    publish(app, snapshot);
    if is_file { wait.min(FILE_POLL) } else { wait }
}

fn publish(app: &AppHandle, snapshot: Snapshot) {
    if let Err(err) = app.emit(EVENT_CHANGED, &snapshot) {
        log::warn!("failed to send the calendar events: {err}");
    }
    *app.state::<Calendar>().snapshot.lock().unwrap() = snapshot;
}

/// The system time zone and its IANA name, read anew each time. Falls back to chrono's local
/// time, with no name, for a zone ICU cannot name.
fn local_zone() -> (Tz, String) {
    crate::platform::current_time_zone()
        .and_then(|name| name.parse::<chrono_tz::Tz>().ok().map(|zone| (Tz::from(zone), name)))
        .unwrap_or((Tz::LOCAL, String::new()))
}

fn events_around_today(text: &str, zone: Tz) -> Vec<Event> {
    let today = Utc::now().with_timezone(&zone).date_naive();
    let midnight = |date: Option<chrono::NaiveDate>| {
        date.and_then(|date| zone.from_local_datetime(&date.and_time(NaiveTime::MIN)).earliest())
            .map(|at| at.with_timezone(&Utc))
            .unwrap_or_else(Utc::now)
    };
    let from = midnight(today.checked_sub_days(Days::new(DAYS_BEFORE)));
    let to = midnight(today.checked_add_days(Days::new(DAYS_AFTER)));
    ics::events_between(text, from, to, zone)
}

/// Where the events come from, as the user typed it in the settings.
#[derive(Debug, PartialEq)]
enum Source {
    /// A link with its scheme; `webcal://` links are already turned into `https://` ones.
    Web(String),
    /// A link typed without a scheme, such as `calendar.google.com/…`: read over HTTPS, since the
    /// link is a secret. Only a server on this computer is tried over HTTP as well.
    Host(String),
    /// An .ics file on this computer or a network share.
    File(PathBuf),
}

impl Source {
    fn parse(input: &str) -> Result<Self, String> {
        // Explorer's "Copy as path" puts the path in quotes.
        let input = input.trim().trim_matches('"').trim();
        if let Some((scheme, rest)) = input.split_once("://")
            && !scheme.is_empty()
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        {
            return match scheme.to_ascii_lowercase().as_str() {
                "https" | "http" => Ok(Self::Web(input.to_string())),
                // Calendar apps hand out webcal:// links for subscribing; they are served over HTTPS.
                "webcal" | "webcals" => Ok(Self::Web(format!("https://{rest}"))),
                "file" => file_url_path(rest).map(Self::File),
                _ => Err(t("calendar.errors.scheme")),
            };
        }
        let path = Path::new(input);
        if path.is_absolute() {
            return Ok(Self::File(path.to_path_buf()));
        }
        let host = input.split(['/', '?', '#']).next().unwrap_or_default();
        let host_ok = (host.contains('.') || host.starts_with("localhost"))
            && host.chars().all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c));
        if !input.contains('/') && input.to_ascii_lowercase().ends_with(".ics") {
            Err(t("calendar.errors.fullPath"))
        } else if host_ok {
            Ok(Self::Host(input.to_string()))
        } else {
            Err(t("calendar.errors.notALink"))
        }
    }

    /// The text of the feed, checked to be a calendar.
    fn read(&self) -> Result<String, String> {
        let bytes = match self {
            Self::Web(url) => fetch(url).map_err(describe)?,
            Self::Host(rest) => fetch(&format!("https://{rest}"))
                .or_else(|err| {
                    if !may_fall_back_to_http(rest, &err) {
                        return Err(err);
                    }
                    // A local server without HTTPS. If it answers over HTTP at all, its answer explains more.
                    fetch(&format!("http://{rest}")).map_err(|http| if is_connection_error(&http) { err } else { http })
                })
                .map_err(describe)?,
            Self::File(path) => read_file(path)?,
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let start = text.trim_start_matches('\u{feff}').trim_start();
        if start.get(..15).is_some_and(|head| head.eq_ignore_ascii_case("BEGIN:VCALENDAR")) {
            Ok(text)
        } else if matches!(self, Self::File(_)) {
            Err(t("calendar.errors.notACalendarFile"))
        } else {
            Err(t("calendar.errors.webPage"))
        }
    }
}

/// The path in `file:///C:/Users/you/calendar.ics` or `file://server/share/calendar.ics`.
fn file_url_path(rest: &str) -> Result<PathBuf, String> {
    let invalid = || t("calendar.errors.invalidFileLink");
    let decoded = percent_decode_str(rest).decode_utf8().map_err(|_| invalid())?;
    let rest = decoded.strip_prefix("localhost/").map(|rest| format!("/{rest}")).unwrap_or_else(|| decoded.to_string());
    let path = match rest.strip_prefix('/') {
        // `/C:/…`: a drive letter.
        Some(local) if local.as_bytes().get(1) == Some(&b':') => local.to_string(),
        Some(_) => rest,
        // A host name: a network share.
        None => format!("//{rest}"),
    };
    let path = PathBuf::from(path);
    if path.is_absolute() { Ok(path) } else { Err(invalid()) }
}

/// Size and modification time, to notice that a calendar file changed.
type FileStamp = (u64, SystemTime);

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|err| match err.kind() {
        ErrorKind::NotFound => t("calendar.errors.fileNotFound"),
        ErrorKind::PermissionDenied => t("calendar.errors.fileNotAllowed"),
        _ => tf("calendar.errors.fileUnreadable", &[("error", &err)]),
    })?;
    if metadata.is_dir() {
        return Err(t("calendar.errors.folder"));
    }
    if metadata.len() > MAX_FEED_BYTES {
        return Err(too_large());
    }
    fs::read(path).map_err(|err| tf("calendar.errors.fileUnreadable", &[("error", &err)]))
}

fn fetch(url: &str) -> Result<Vec<u8>, ureq::Error> {
    let agent = crate::http::agent(Duration::from_secs(60));
    agent.get(url).call()?.body_mut().with_config().limit(MAX_FEED_BYTES).read_to_vec()
}

/// True when the server could not be reached at all, as opposed to answering with an error.
fn is_connection_error(err: &ureq::Error) -> bool {
    !matches!(err, ureq::Error::StatusCode(_) | ureq::Error::BodyExceedsLimit(_))
}

/// True when a link typed without a scheme may be read over plain HTTP after HTTPS failed with
/// `err`. Only a server on this computer may: elsewhere, anyone on the network could make HTTPS
/// fail, a certificate check for one, and read the secret link from the plain request. Other
/// servers without HTTPS need an explicit `http://` link.
fn may_fall_back_to_http(rest: &str, err: &ureq::Error) -> bool {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once(':').map_or(authority, |(host, _)| host);
    let local = host.eq_ignore_ascii_case("localhost") || host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback());
    local && is_connection_error(err)
}

fn describe(err: ureq::Error) -> String {
    match err {
        ureq::Error::StatusCode(401 | 403) => t("calendar.errors.refused"),
        ureq::Error::StatusCode(404) => t("calendar.errors.notFound"),
        ureq::Error::StatusCode(code) => tf("calendar.errors.httpError", &[("code", &code)]),
        ureq::Error::BodyExceedsLimit(_) => too_large(),
        err => {
            log::warn!("could not download the calendar feed: {}", without_link(&err));
            t("calendar.errors.unreachable")
        }
    }
}

/// What went wrong, for the log. Some errors quote the URL, which is the secret link, so only
/// those known not to are written out.
fn without_link(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Io(err) => format!("io: {:?}", err.kind()),
        ureq::Error::Timeout(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Tls(_)
        | ureq::Error::NativeTls(_)
        | ureq::Error::Der(_)
        | ureq::Error::TlsRequired
        | ureq::Error::TooManyRedirects
        | ureq::Error::RedirectFailed
        | ureq::Error::LargeResponseHeader(..)
        | ureq::Error::BodyStalled => err.to_string(),
        ureq::Error::BadUri(_) => "the link is not a valid URL".into(),
        // A redirect's Location header can be quoted.
        ureq::Error::Protocol(_) => "HTTP protocol error".into(),
        _ => "unexpected error".into(),
    }
}

fn too_large() -> String {
    tf("calendar.errors.tooLarge", &[("size", &(MAX_FEED_BYTES / 1024 / 1024))])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn reads_the_system_time_zone() {
        let (zone, name) = local_zone();
        assert!(!name.is_empty(), "the zone of this computer should have an IANA name");
        let now = Utc::now();
        assert_eq!(now.with_timezone(&zone).naive_local(), now.with_timezone(&chrono::Local).naive_local());
    }

    #[test]
    fn settings_are_valid() {
        crate::widget_settings::validate(&settings()).unwrap();
    }

    #[test]
    fn reads_links_with_and_without_a_scheme() {
        let web = |url: &str| Ok(Source::Web(url.to_string()));
        assert_eq!(Source::parse("webcal://p01-caldav.icloud.com/x"), web("https://p01-caldav.icloud.com/x"));
        assert_eq!(Source::parse("WEBCAL://a.com/b.ics"), web("https://a.com/b.ics"));
        assert_eq!(Source::parse(" https://a.com/b.ics "), web("https://a.com/b.ics"));
        assert_eq!(Source::parse("http://a.com/b.ics"), web("http://a.com/b.ics"));
        let host = |rest: &str| Ok(Source::Host(rest.to_string()));
        let google = "calendar.google.com/calendar/ical/x/basic.ics";
        assert_eq!(Source::parse(google), host(google));
        assert_eq!(Source::parse("localhost:8080/cal.ics"), host("localhost:8080/cal.ics"));
        assert_eq!(Source::parse("a.com"), host("a.com"));
        for bad in ["ftp://a.com/b.ics", "calendar", "meeting.ics", "two words.com", ""] {
            assert!(Source::parse(bad).is_err(), "{bad} should be refused");
        }
    }

    #[cfg(windows)]
    #[test]
    fn reads_file_paths_and_links() {
        let file = |path: &str| Ok(Source::File(PathBuf::from(path)));
        let downloads = r"C:\Users\me\Downloads\meeting (2).ics";
        assert_eq!(Source::parse(downloads), file(downloads));
        assert_eq!(Source::parse(r#""C:\Users\me\cal.ics""#), file(r"C:\Users\me\cal.ics"));
        assert_eq!(Source::parse("D:/calendars/work.ics"), file("D:/calendars/work.ics"));
        assert_eq!(Source::parse(r"\\server\share\cal.ics"), file(r"\\server\share\cal.ics"));
        assert_eq!(Source::parse("file:///C:/Users/me/My%20Calendar.ics"), file("C:/Users/me/My Calendar.ics"));
        assert_eq!(Source::parse("file://localhost/C:/cal.ics"), file("C:/cal.ics"));
        assert_eq!(Source::parse("file://server/share/cal.ics"), file("//server/share/cal.ics"));
        assert!(Source::parse("file:///relative.ics").is_err());
    }

    #[test]
    fn falls_back_to_http_only_on_this_computer() {
        let refused = ureq::Error::ConnectionFailed;
        let certificate = ureq::Error::Tls("invalid certificate");
        assert!(may_fall_back_to_http("localhost:8080/cal.ics", &refused));
        assert!(may_fall_back_to_http("LOCALHOST/cal.ics", &certificate));
        assert!(may_fall_back_to_http("127.0.0.1:5232/user/cal.ics", &certificate));
        assert!(!may_fall_back_to_http("localhost:8080/cal.ics", &ureq::Error::StatusCode(404)));
        for remote in ["calendar.google.com/calendar/ical/x/basic.ics", "localhost.example.com/cal.ics", "10.0.0.2/cal.ics"]
        {
            assert!(!may_fall_back_to_http(remote, &refused), "{remote}");
            assert!(!may_fall_back_to_http(remote, &certificate), "{remote}");
        }
    }

    #[test]
    fn logs_errors_without_the_link() {
        let secret = "calendar.google.com/calendar/ical/secret%40group/private-0123/basic.ics";
        let errors = [
            ureq::Error::BadUri(format!("https://{secret}")),
            ureq::Error::RequireHttpsOnly(format!("http://{secret}")),
            ureq::Error::Io(std::io::Error::other(format!("https://{secret}"))),
            fetch(&format!("https://exa mple.com/{secret}")).unwrap_err(),
        ];
        for err in &errors {
            let logged = without_link(err);
            assert!(!logged.contains("private-0123"), "{logged}");
        }
        assert_eq!(without_link(&ureq::Error::ConnectionFailed), "connection failed");
    }

    #[test]
    fn reads_a_local_server_without_https() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            // The TLS handshake comes first and gets no answer.
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut request = [0u8; 4096];
                let read = stream.read(&mut request).unwrap_or(0);
                if request[..read].starts_with(b"GET ") {
                    let body = "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n";
                    let reply = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    stream.write_all(reply.as_bytes()).unwrap();
                }
            }
        });
        assert!(Source::Host(format!("127.0.0.1:{port}/cal.ics")).read().is_ok());
        server.join().unwrap();
    }

    #[test]
    fn reads_calendar_files() {
        let dir = std::env::temp_dir().join(format!("lizun-calendar-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let calendar = dir.join("cal.ics");
        fs::write(&calendar, "\u{feff}BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n").unwrap();
        assert!(Source::File(calendar.clone()).read().is_ok());
        let stamp = file_stamp(&calendar);
        assert!(stamp.is_some());
        fs::write(&calendar, "BEGIN:VCALENDAR\r\nX-CHANGED:1\r\nEND:VCALENDAR\r\n").unwrap();
        assert_ne!(file_stamp(&calendar), stamp);

        let page = dir.join("page.ics");
        fs::write(&page, "<html></html>").unwrap();
        assert!(Source::File(page).read().unwrap_err().contains("not a calendar"));
        assert!(Source::File(dir.join("missing.ics")).read().unwrap_err().contains("cannot find"));
        assert!(Source::File(dir.clone()).read().unwrap_err().contains("folder"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Reads Google's public calendar of US holidays; run with `cargo test -- --ignored` when
    /// online.
    #[test]
    #[ignore]
    fn reads_a_google_feed() {
        let url = "https://calendar.google.com/calendar/ical/en.usa%23holiday%40group.v.calendar.google.com/public/basic.ics";
        let text = Source::Web(url.into()).read().unwrap();
        let now = Utc::now();
        let events =
            ics::events_between(&text, now - chrono::TimeDelta::days(200), now + chrono::TimeDelta::days(200), Tz::LOCAL);
        assert!(events.iter().any(|event| event.all_day && !event.title.is_empty()), "{events:?}");
        let page = Source::parse("calendar.google.com").unwrap().read().unwrap_err();
        assert!(page.contains("web page"), "{page}");
    }
}
