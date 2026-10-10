# ADR 0012: Seeking, system load, links, clipboard, user addresses and secrets for widgets

Status: accepted (2026-10-10). How widgets use it is described in
[widgets.md](../widgets.md#permissions).

## Context

The second batch of widgets (Pomodoro, Lyrics, Weather, Habits, System monitor, IP location)
and the ones people asked for next (Home Assistant, RSS, OBS, GitHub) ran into the same gaps:

- Lyrics should jump to a line when it is clicked; the SDK could only play, pause and skip.
- A system monitor had no way to see the processor, memory or network load from the sandbox.
- A widget could not open a link (an article, a pull request) or copy a value (an IP address).
- `network` is fixed in the manifest, so a widget could not talk to a server only the user
  knows: their Home Assistant, a feed, a NAS. Those usually live on the local network over plain
  HTTP, which was allowed only to this computer.
- Tokens for services such as GitHub would have been kept in plain text in
  `widget-settings.json`.
- The island fades its panel when it closes, so the widget pages never get
  `visibilitychange` and cannot tell whether anyone looks at them.

## Decision

- **`media.seek(positionMs)`** under `media:control`, with `canSeek` in now playing. System
  sessions use SMTC's `TryChangePlaybackPositionAsync` when the session says the position can be
  changed; browser tabs use the extension's existing `seek` command. The built-in media widget's
  progress bar seeks on click too.
- **`system.stats()`** under a new `system` permission (`src-tauri/src/system_stats.rs`,
  Windows part in `platform/win/system.rs`). It is built on Win32 without a new dependency:
  PDH counters for "% Processor Utility" (overall and per processor) and "GPU Engine
  Utilization", the way Task Manager reads them; `GlobalMemoryStatusEx`, `GetSystemPowerStatus`,
  the fixed drives, and the byte counters of hardware network adapters from `GetIfTable2`, so a
  VPN's virtual adapter does not count the same traffic twice. One sampler serves all widgets
  and hands out the same sample for 500 ms. Temperatures are left out: on Windows they need a
  kernel driver and administrator rights; the System monitor widget reads them from
  LibreHardwareMonitor's own web server when the user runs it.
- **`links.open(url)`** under `links` and **`clipboard.writeText(text)`** under
  `clipboard:write` (`src-tauri/src/widget_actions.rs`). Opening links could carry data off the
  `network` list and spam tabs, and writing the clipboard can replace what the user copied, so
  both follow the keyboard rule: only while the island is open and the pointer is on the widget,
  that is, from a click. The core takes only `http`/`https` links with a host (no `file:`,
  `ms-settings:` or other shell handlers), up to 1 MB of text, and each action at most twice a
  second per widget.
- **`url` settings.** The user types an address; the widget may reach its origin by `fetch`,
  WebSocket and `network.request`, as if it were in `network`. The user chose it, which is the
  same consent the install review gives for the manifest's list, and the widget cannot write
  settings, so it cannot pick the address itself. `https://` is taken anywhere; `http://` only
  for this computer (minus the refused ports) and the local network: RFC 1918 and link-local
  IPv4, `100.64.0.0/10` (Tailscale and other mesh VPNs), IPv6 unique-local and link-local, and
  names without a dot or ending in `.local`, `.lan`, `.home`, `.home.arpa`, `.internal`. Values
  are stored normalized, so the user sees exactly what is reached. The CSP is per response, so
  the island starts the widget's frame over when such an address changes. Settings and the
  install review show these widgets as "Talks to the addresses you enter in its settings".
- **WebSockets go to the same origins.** CSP Level 3 says an `https://` source also matches
  `wss://` on the same host and port, and `http://` matches `ws://`, but WebView2 blocked
  `ws://127.0.0.1:8899` with only `http://127.0.0.1:8899` in `connect-src` (tried 2026-10-10).
  So the widget's CSP lists each origin twice, as `https://`/`http://` and as `wss://`/`ws://`.
  The socket is opened by the page and sends `Origin: null`.
- **`secret` settings** are encrypted with DPAPI for the current Windows user and stored as
  `{"$protected": "<base64>"}` in `widget-settings.json`. The settings window shows a password
  field that never holds the saved value; the widget gets the plain text, as it must to use it.
  A secret that cannot be decrypted (a file from another computer or user) reads as empty.
- **`island.subscribe`** and `islandOpen` in `ready()`: the island announces when it opens and
  closes, so widgets refresh when they are looked at and stop polling when they are not.
- **Long storage keys** (more than 100 UTF-8 bytes, such as 64 Cyrillic letters) were named by
  their hex and went past the 255 characters of a file name; they are now named by their
  SHA-256 after a `~`. Shorter keys keep their file names, so stored data is not moved.

## Consequences

- Widgets for Home Assistant, RSS, OBS (obs-websocket), a NAS or GitHub can be written without
  changes to the core; GitHub-like ones keep their tokens encrypted.
- The local network becomes reachable for a widget only through an address the user typed. A
  name such as `nas` is resolved by the system, so it reaches whatever the network calls `nas`;
  that is what the user asked for.
- `system` shows how busy the computer is, not what runs on it; process lists stay out of
  reach.
- Windows only, like the rest of the platform layer; other platforms report these calls as
  unsupported.
