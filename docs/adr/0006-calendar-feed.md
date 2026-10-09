# ADR 0006: Calendar events from an iCal link or file

Status: accepted (2026-10-09).

## Context

The calendar widget showed the week around today and a fixed "Nothing for today": it had no
source of events. People keep their calendars in Google, Outlook (Microsoft 365 or
Outlook.com) and iCloud. Windows used to gather such accounts in its Calendar app, but that app
was retired in favour of the new Outlook, which keeps its data to itself.

## Decision

- **The user pastes the calendar's secret iCal (.ics) address** into the widget's settings
  (Settings > Widgets > Calendar). Google, Outlook and iCloud all offer one, and it needs no
  sign-in, no app registration and no tokens. `webcal://` links are read over HTTPS, and a link
  typed without a scheme is tried over HTTPS, then over HTTP if the server cannot be reached.
- **An .ics file works too**: a path such as `C:\Users\you\calendar.ics` (with or without the
  quotes Explorer's "Copy as path" adds), a network share or a `file://` link. This covers
  invitations exported from Zoom or Teams and saved from mail. The file is checked for changes
  every 10 seconds and read again when its size or modification time changes. A "Choose…"
  button next to the field picks the file in a dialog: text settings can declare `file` (a
  name and extensions) for that, which only built-in widgets may do, since an installed widget
  cannot read files and the path would tell it about the user's folders.
- **The core downloads and reads the feed** (`src-tauri/src/calendar.rs`, parser in
  `src-tauri/src/ics.rs`). A background thread downloads it when the link changes, then every
  15 minutes by default (5 to 240, a setting); opening the island also refreshes a feed older
  than that, since the thread's timer does not run while the computer sleeps. Recurring events
  are expanded with the `rrule` crate for the weeks around today, and only those events go to
  the island, through `calendar_snapshot` and the `calendar://changed` event.
- **Time zones** are read by IANA name; Windows names, which Outlook uses, are mapped with the
  ICU that ships with Windows. Floating times and unknown zones count as local time.
- **Local time is the system time zone at the moment**, read from Windows on every use and
  sent to the island with the events (`timeZone`). The WebView keeps the zone it started with,
  so the widget counts days and formats times in the reported zone instead of with `Date`. After
  the user changes the zone, the next read lays the events out again.
- **The calendar is the first built-in widget with settings.** Its schema is declared in Rust
  and goes through the same store, checks and settings screen as the settings of installed
  widgets (ADR 0002); the settings window gets it from `widget_settings_built_in`.
- If a download fails, the island keeps the events it has and shows a warning sign by the month,
  and the next try comes after two minutes.

## Alternatives considered

| Option | Why not |
|---|---|
| Google Calendar API and Microsoft Graph with OAuth | Live data and writing events, but an app registration with each service, sign-in in a browser window, token storage and refresh, and a separate integration per service. Can come later for people who want to add events from the island. |
| Windows `AppointmentStore` (WinRT) | Reads the accounts of the retired Windows Calendar app; the new Outlook does not fill it, and it expects an app with package identity (MSIX). |
| Reading VTIMEZONE definitions | Every service also names its zones with a known IANA or Windows name, so the extra parser would only help unusual exporters. |
| Doing it in the widget's web code | The island page cannot fetch other origins without CORS headers, which calendar services do not send, and the secret link would be handled by page code. |

## Consequences

- Read only: events cannot be added or changed from the island.
- Google refreshes its secret feed only every few hours, so a new event can take that long to
  show up. Outlook and iCloud are quicker.
- One link per widget for now. Several calendars at once need either several links or a merged
  feed.
- The link gives anyone who has it read access to the calendar. Lizun keeps it in
  `widget-settings.json` in the app config folder and never logs it; the settings window shows it
  in a plain text field.
- The ICU lookup needs `icuin.dll`, part of Windows 10 1703 and later.
