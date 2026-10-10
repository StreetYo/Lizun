# Widget catalog

Widgets for Lizun that you can install. Each one lives in its own repository; this page only
lists them.

## Installing a widget

1. Copy the widget's link from the table below.
2. In Lizun, open Settings > Widgets > Add a widget, paste the link and press Install.
3. Lizun checks the widget and shows its permissions and network access; it is installed only
   after you confirm.

The same link updates a widget that is already installed; its settings and data are kept.

## Widgets

| Widget | What it does | Permissions | Link |
|---|---|---|---|
| Anki | Review flash cards: your Anki decks through AnkiConnect, or decks kept in the widget | `storage`, network `http://127.0.0.1:8765`, starts Anki | [Lizun-anki](https://github.com/StreetYo/Lizun-anki) |
| Habits | Daily habits with check-offs, streaks and the last week at a glance | `storage`, `activity` | [lizun-habits](https://github.com/StreetYo/lizun-habits) |
| IP location | Where the internet sees you: public IP, country, city and provider, to check a VPN | `storage`, `activity`, `clipboard:write`, network `ipwho.is`, `ipapi.co`, `ipinfo.io`, `api6.ipify.org` | [lizun-ip-location](https://github.com/StreetYo/lizun-ip-location) |
| Lyrics | Synced lyrics of the current track from LRCLIB; click a line to jump to it | `media:read`, `media:control`, `storage`, network `lrclib.net` | [lizun-lyrics](https://github.com/StreetYo/lizun-lyrics) |
| Notepad | A note you can write and edit right in the island | `storage` | [lizun-notepad](https://github.com/StreetYo/lizun-notepad) |
| Pomodoro | Focus and break timer with the countdown in the closed island; can pause music on breaks | `storage`, `activity`, `media:read`, `media:control` | [lizun-pomodoro](https://github.com/StreetYo/lizun-pomodoro) |
| System monitor | Processor, graphics, memory, disk, network and battery load; temperatures from LibreHardwareMonitor | `system`, `activity`, network `http://localhost:8085` | [lizun-system-monitor](https://github.com/StreetYo/lizun-system-monitor) |
| Weather | Current weather and the next hours from Open-Meteo, for a city you pick | `storage`, `activity`, network `api.open-meteo.com`, `geocoding-api.open-meteo.com` | [lizun-weather](https://github.com/StreetYo/lizun-weather) |

Permissions: `storage` keeps the widget's data on this computer, `media:read` and
`media:control` see and control what is playing, `activity` shows something in the closed
island, `system` sees how busy the computer is, `links` opens links in the browser and
`clipboard:write` copies text, the last two only when you click the widget. A widget may also
reach the network, only the sites its manifest lists and the addresses you enter in its
settings, and start programs its manifest names; Lizun shows them before installing.

## Adding your widget

1. Write the widget as described in [Writing a Lizun widget](widgets.md) and put it in a public
   GitHub repository, with `manifest.json` at the root.
2. Make sure it installs from the repository link and works in English (translations are
   welcome).
3. Its README says what the widget does and why it needs each permission and network site.
4. Open a pull request that adds one row to the table above, in alphabetical order.

A widget is removed from the list when it stops installing or does something its README does not
say.
