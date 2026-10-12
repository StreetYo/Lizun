# Writing a Lizun widget

A widget is a folder with a `manifest.json` and web files. It is shown as a column in the open
island, inside a sandboxed iframe.

```
my-widget/
  manifest.json
  index.html
  main.js
  style.css
```

## manifest.json

```json
{
  "id": "com.example.weather",
  "name": "Weather",
  "version": "1.0.0",
  "author": "You",
  "description": "Current weather",
  "entry": "index.html",
  "width": 240,
  "permissions": ["storage"],
  "network": ["https://api.example.com"]
}
```

| Field | Meaning |
|---|---|
| `id` | 3–64 lowercase letters, digits, dots or dashes. Unique. |
| `name` | Up to 50 characters. |
| `version` | Up to 32 letters, digits, dots, dashes, pluses or `_`, such as `1.2.0`. |
| `author`, `description` | Optional, up to 60 and 200 characters. |
| `entry` | HTML file to load (default `index.html`); it must end in `.html` or `.htm`. |
| `width` | Column width in logical pixels, 120–640 (default 240). |
| `permissions` | `media:read`, `media:control`, `storage`, `activity`, `system`, `links`, `clipboard:write`, each listed once. Anything else is rejected. See [Permissions](#permissions). |
| `network` | Origins the widget may reach, each listed once: `https://host[:port]`, or `http://127.0.0.1:port` and `http://localhost:port` for a program on this computer. `https://*.example.com` covers the subdomains; a wildcard needs at least two labels after it, so `*.com` is rejected. Empty means no network. See [Network](#network). |
| `apps` | Programs the widget may start, see [Starting a program](#starting-a-program). |
| `settings` | Options the user can change in Settings > Widgets, see [Settings](#settings). |
| `locales` | Translations of `name`, `description` and the settings, see [Languages](#languages). |

Texts the user sees (`name`, `author`, `description`, the settings' labels and so on, and their
translations) may not contain control characters, line separators or the invisible characters
that change the direction of text (U+200E, U+200F, U+061C, U+2028–U+202E, U+2066–U+2069), so a
widget cannot make its name look like something else.

## The SDK

Load the shared stylesheet and import the SDK from the island:

```html
<link rel="stylesheet" href="/__sdk/lizun.css" />
<script type="module" src="./main.js"></script>
```

```js
import { ready, media, storage } from '/__sdk/index.js';

const context = await ready(); // connects and applies the island theme (--lz-* CSS variables)

media.subscribe((nowPlaying) => { /* needs media:read */ });
await media.togglePlayPause(); // needs media:control

const count = (await storage.get('count')) ?? 0; // needs storage
await storage.set('count', count + 1);
```

Calls without the needed permission reject with a `LizunError` whose `code` is
`permission_denied`. Types are in `packages/widget-sdk/src/index.ts`.

### Permissions

| Permission | Lets the widget |
|---|---|
| `media:read` | See what is playing: `media.getNowPlaying()`, `media.subscribe()`. |
| `media:control` | Play, pause, skip and seek: `media.togglePlayPause()`, `next()`, `previous()`, `seek(positionMs)`. |
| `storage` | Keep data, see [Storage](#storage). |
| `activity` | Show something in the closed island, see [Live activity](#live-activity). |
| `system` | Read the computer's load, see [System load](#system-load). |
| `links` | Open web links in the browser, see [Links and the clipboard](#links-and-the-clipboard). |
| `clipboard:write` | Put text on the clipboard, same section. |

### Media

`media.subscribe` calls the listener when the track, its state or its position changes, not
every second: between events, advance `positionMs` yourself while `playing` is true.
`media.seek(positionMs)` jumps within the current track when `canSeek` is true; players without
a timeline, such as live streams, cannot seek.

### Island open or closed

Closing the island fades its panel but does not hide the widget pages, so `visibilitychange`
does not fire in them. `ready()` returns `islandOpen`, and `island.subscribe` reports changes:

```js
import { island, ready } from '/__sdk/index.js';

const context = await ready();
let open = context.islandOpen;
island.subscribe((state) => {
  open = state.open;
  if (open) refreshIfStale();
});
```

Refresh what the user is about to look at when the island opens, and stop polling while it is
closed unless the widget's live activity needs the data.

## Storage

With the `storage` permission a widget keeps JSON values that survive restarts and updates:

```js
import { storage } from '/__sdk/index.js';

await storage.set('deck', { name: 'Words', cards: [] });
const deck = await storage.get('deck'); // null when never set
await storage.remove('deck');
```

Keys are up to 128 characters. A value is up to 1 MB of JSON and a widget keeps up to 50 MB in
total; a `set` past either limit rejects with a `LizunError`. Data that can grow, such as a list
of thousands of items, is best split over several keys, so that a change rewrites only one of
them.

Values are files in `%APPDATA%\app.lizun.desktop\widget-data\<id>\`, on this computer only.
Removing a widget keeps them for a later reinstall. Earlier Lizun builds kept them in the
island's `localStorage`; they move to files before the widget's first storage call.

## Network

A widget reaches only the origins in its manifest's `network` list, which the user sees before
installing. It can use `fetch`, which works like in a browser: the iframe's origin is opaque, so
the server has to allow any origin with CORS. Or it can ask Lizun to send the request with
`network.request`, which is not limited by CORS and sends no `Origin` header, so it also
reaches APIs made for apps, such as an app on this computer:

```js
import { network } from '/__sdk/index.js';

const response = await network.request('http://127.0.0.1:8765', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ action: 'version', version: 6 }),
});
if (response.ok) console.log(response.json());
```

- `method` is `GET` (the default), `HEAD`, `POST`, `PUT`, `PATCH` or `DELETE`; `headers` cannot
  set `Host`, `Origin`, `Content-Length`, `Connection` or `Transfer-Encoding`.
- The body goes and comes back as text: up to 16 MB out and 32 MB back. The answer has
  `status`, `ok`, `headers` (by lowercase name), `body` and `json()`.
- Redirects are not followed, so a 3xx answer comes back as it is; a request takes at most a
  minute.
- An HTTP error such as 404 is an answer; the call rejects with a `LizunError` only when the
  address is not in `network`, the server cannot be reached, the time runs out or the widget
  already has four requests in flight.

Plain HTTP is allowed only to `127.0.0.1` and `localhost`, with a port. Settings show such an
origin as talking to a program on this computer. Ports where a debugger, Docker or Lizun itself
listens (2375, 2376, 6869, 9222, 9229 and Lizun's browser extension port) are refused. Why it
works this way is in [ADR 0010](adr/0010-widget-network-and-storage.md).

**WebSockets** go to the same origins: an `https://` origin also allows `wss://` to the same host
and port, and an `http://` one allows `ws://`, so `new WebSocket('ws://127.0.0.1:4455')` works
with `http://127.0.0.1:4455` in `network`. The widget's page opens the socket itself, so it sends
`Origin: null`; a server that checks the origin has to accept that.

**Addresses the user enters.** A widget for a server only the user knows, such as their Home
Assistant, an RSS feed or a NAS, declares a [`url` setting](#settings) instead of listing the
server in `network`. The address the user types there can be reached as if it were in `network`,
by `fetch`, WebSocket and `network.request`, including plain HTTP on the local network: private
addresses such as `192.168.1.5` and `10.0.0.2`, `100.64.0.0/10` used by mesh VPNs such as
Tailscale, local IPv6 addresses, and names such as `nas` or `homeassistant.local`. Settings
show such a widget as "Talks to the addresses you enter in its settings". When the user changes
the address, the widget starts over so its page gets the new one.

## Starting a program

A widget that works with a program on this computer, such as Anki, can start it. The manifest
lists the program with the paths it may be installed at, tried in order:

```json
{
  "apps": [
    {
      "id": "anki",
      "name": "Anki",
      "paths": ["%LOCALAPPDATA%\\Programs\\Anki\\anki.exe", "%PROGRAMFILES%\\Anki\\anki.exe"]
    }
  ]
}
```

A path is an `.exe` under `%LOCALAPPDATA%`, `%APPDATA%`, `%PROGRAMFILES%`, `%PROGRAMFILES(X86)%`
or a drive letter, without `..`, other variables or arguments; up to four programs and eight
paths each. Settings show "Starts Anki" with the widget's permissions.

```js
import { apps } from '/__sdk/index.js';

const result = await apps.start('anki', { hidden: true }); // 'started', 'running' or 'notFound'
await apps.show('anki'); // shows its window and brings it to the front; false if there is none
```

- A program that is already running is not started again.
- With `hidden`, it starts in the background: its windows are hidden while it starts up (the
  first 30 seconds), so it has no window or taskbar button. Opening it by hand later, or
  `apps.show`, shows it.
- When Lizun quits, a program it started in the background that was never shown is asked to
  close, as if its close button had been pressed.
- Windows only for now; elsewhere `apps.start` rejects.

Start a program when the user needs it, for example when the widget loads and is set to use it,
not on every pointer move: someone who closed it by hand expects it to stay closed. Why it works
this way is in [ADR 0011](adr/0011-widgets-starting-apps.md).

## Settings

A widget does not build its own settings screen. It lists its options in the manifest, and
Lizun shows them in Settings > Widgets under the widget, checks and stores the values, and hands
them to the widget. Why it works this way is in [ADR 0002](adr/0002-widget-settings.md).

```json
{
  "id": "com.example.weather",
  "settings": [
    { "key": "city", "type": "text", "label": "City", "default": "Moscow", "maxLength": 80 },
    {
      "key": "units",
      "type": "select",
      "label": "Units",
      "default": "metric",
      "options": [
        { "value": "metric", "label": "Celsius" },
        { "value": "imperial", "label": "Fahrenheit" }
      ]
    },
    { "key": "refresh", "type": "number", "label": "Refresh every", "unit": "min", "default": 15, "min": 5, "max": 120 },
    { "key": "feelsLike", "type": "toggle", "label": "Show \"feels like\"", "default": true }
  ]
}
```

Every setting has these fields:

| Field | Meaning |
|---|---|
| `key` | Name the widget reads the value by: a letter, then up to 31 letters, digits or `_`. Unique within the widget. |
| `type` | `toggle`, `number`, `select`, `text`, `url` or `secret`. |
| `label` | Shown next to the control, up to 60 characters. |
| `description` | Optional hint under the label, up to 200 characters. |
| `default` | The value until the user changes it; it must be valid for the type. Required, except for `url` (`""` when left out) and `secret` (which has none). |

Types and their extra fields:

| Type | Value | Extra fields |
|---|---|---|
| `toggle` | `true` / `false` | none |
| `number` | number | `min` and `max` (required), `step` (default 1), `unit` (optional label such as `s` or `min`, up to 10 characters) |
| `select` | one of the option values | `options`: 2–20 `{ "value": string, "label": string }`; values are 1–64 characters and unique, labels up to 60 |
| `text` | string | `maxLength` (1–500, default 100), `placeholder` (optional, up to 100 characters) |
| `url` | `""` or the address as Lizun normalized it, such as `http://192.168.1.5:8123/` | `placeholder` (optional) |
| `secret` | `""` until set, then the text the user entered (up to 4 KB) | `placeholder` (optional) |

A `url` setting takes `https://` addresses anywhere, and `http://` only on this computer and the
local network (see [Network](#network)); an address typed without a scheme is read as
`https://`. Lizun refuses anything else when the user enters it, and the widget may reach the
address's origin. A `default` has to be written the way Lizun normalizes it, such as
`https://example.com/`.

A `secret` setting is for tokens and passwords. The settings window shows a password field and
never shows the saved value: typing replaces it, and a button clears it. It is stored encrypted
for the Windows user (DPAPI), so it cannot be read from a copied or synced settings file. The
widget gets it in plain text.

A widget may declare up to 32 settings, shown in the manifest order. A manifest whose settings
break these rules is rejected like any other invalid manifest, and the reason is logged.

How values are kept:

- Values live in `widget-settings.json` in the Lizun config folder
  (`%APPDATA%\app.lizun.desktop\`), per widget id, not inside the widget folder, so they
  survive widget updates and reinstalls. Settings > Widgets has a "Reset to defaults" button
  for each widget.
- When the widget changes its `settings`, saved values are checked against the new list: a
  number out of range is clamped, text longer than `maxLength` is cut, a value of the wrong type
  or a removed option falls back to `default`, and keys that are no longer declared are left
  out.
- Only the user changes settings. A widget reads them and cannot write them; data the widget
  writes itself goes to `storage`.

Reading them needs no permission:

```js
import { ready, settings } from '/__sdk/index.js';

const context = await ready();
let options = context.settings; // { city: 'Moscow', units: 'metric', refresh: 15, feelsLike: true }

// Called with all values whenever the user changes any of them in Settings.
settings.subscribe((next) => {
  options = next;
  render();
});
```

`settings.getAll()` returns the same object as a promise. Values always have the declared type,
so the widget does not need to check them.

The example widget in `widgets/hello` declares one setting of each type.

Lizun builds that predate settings reject a manifest with a `settings` field.

## Languages

Lizun's interface comes in several languages (Settings > General > Language). A widget follows
it in two ways.

**Texts in the manifest** are translated by Lizun. Add `locales` with the texts for each
language code; anything left out stays as written in the manifest:

```json
{
  "name": "Weather",
  "description": "Current weather",
  "settings": [
    {
      "key": "units", "type": "select", "label": "Units", "default": "metric",
      "options": [{ "value": "metric", "label": "Celsius" }, { "value": "imperial", "label": "Fahrenheit" }]
    },
    { "key": "refresh", "type": "number", "label": "Refresh every", "unit": "min", "default": 15, "min": 5, "max": 120 }
  ],
  "locales": {
    "ru": {
      "name": "Погода",
      "description": "Погода сейчас",
      "settings": {
        "units": { "label": "Единицы", "options": { "metric": "Цельсий", "imperial": "Фаренгейт" } },
        "refresh": { "label": "Обновлять каждые", "unit": "мин" }
      }
    }
  }
}
```

A setting's translation can have `label`, `description`, `placeholder` (text settings), `unit`
(number settings) and `options` (select settings, labels by option value), with the same
length limits as in `settings`. Keys of settings or options that do not exist are rejected.
Codes are language tags such as `ru` or `pt-BR`; `pt-BR` is also used when Lizun is in `pt`.

**Texts in the widget's page** are the widget's own. `ready()` returns the language code, sets
it as the page's `lang`, and `language.subscribe()` reports a switch:

```js
import { language, ready } from '/__sdk/index.js';

const TEXTS = {
  en: { empty: 'Nothing to do' },
  ru: { empty: 'Дел нет' },
};

const context = await ready();
let text = TEXTS[context.language] ?? TEXTS.en;
language.subscribe((next) => {
  text = TEXTS[next] ?? TEXTS.en;
  render();
});
```

The bundled To-do widget (`apps/desktop/bundled-widgets/todo`) does both.

## Live activity

While the island is closed, a widget can show something in the mini island: a timer counting
down, the next meeting, the next task. It needs the `activity` permission.

The mini island is a capsule as wide as its content, with something at each end and a line of
text in the middle. The widget does not draw it: it describes the ends with an icon from a fixed
set and a short text, adds the line in the middle, and the island draws them in its own style:

```js
import { activity } from '/__sdk/index.js';

await activity.set({
  leading: { icon: 'timer', color: '#ff9f0a' },
  title: 'Tea',
  trailing: { text: '2:40' },
});

await activity.clear(); // nothing to show
```

| Field | Meaning |
|---|---|
| `leading`, `trailing` | Left and right end. |
| `title` | One line in the middle, up to 100 characters. The capsule grows with it up to a limit, then the text is cut with an ellipsis. |
| `icon` | One of `alarm`, `battery`, `bell`, `calendar`, `chat`, `check-circle`, `clock`, `cloud`, `coffee`, `download`, `envelope`, `fire`, `flag`, `headphones`, `heart`, `hourglass`, `info`, `lightning`, `list-checks`, `map-pin`, `microphone`, `moon`, `note`, `package`, `phone`, `record`, `star`, `sun`, `target`, `timer`, `trophy`, `upload`, `video-camera`, `warning`. |
| `text` | Text at an end, up to 16 characters; what does not fit is cut with an ellipsis, so keep it to a number or a word. |
| `color` | `#rrggbb` for the icon and text of that end; the island's text color when left out. `context.theme.textSecondary` gives a dimmer look. |

At least one field must show something. Anything else is rejected with a `LizunError`.

On screens with a camera notch (macOS, when Lizun gets there) the middle of the island is
covered, so `title` will not be shown there and the ends will sit on either side of the notch.
Keep the ends meaningful on their own.

How it is shown:

- The mini island shows one thing at a time: the current track first, then files in the tray,
  then widget activities in the order the widgets have in the island (the first row, then the
  second). The user sets the priority by moving widgets, and a hidden widget shows nothing.
- When nothing has an activity, the mini island shows a pair of eyes, or, with the eyes turned
  off in the settings, goes into the top edge; pointing at the edge still opens the island.
- In "Hide after a delay" mode the mini island comes out when a widget's activity appears, not
  on every `set`, so a ticking timer does not keep it on screen.
- `set` replaces the widget's previous activity. It is kept while the widget's frame reloads
  (the island reloads widgets, for example after the tray tab was open), so a widget should set
  or clear it every time it starts.

The To-do widget is an example: it shows the next open task and how many more are left, and
has a setting to turn that off.

## Text input

The island never takes the keyboard on its own, so the app you work in keeps it. The SDK
changes that only while a text field in the widget is in use: clicking an `<input>`,
`<textarea>` or `contenteditable` element brings the island to the foreground, and it stays
open even if the cursor leaves. The keyboard goes back to the previously active window when
the field loses focus, when Esc is pressed in it, when the user clicks anything else, or when
the island closes. Nothing needs to be declared for this.

The island hands the keyboard only to the widget the user is working with: while the island is
open and the pointer is on that widget. A field the widget focuses on its own at other times
(with `autofocus` or `focus()`) does not get the keyboard, and the request is refused with a
`LizunError` whose `code` is `not_allowed`.

To hand the keyboard back yourself, for example after a form is submitted:

```js
import { keyboard } from '/__sdk/index.js';

keyboard.release();
```

The bundled To-do widget (`apps/desktop/bundled-widgets/todo`) is a complete example: Enter adds
a task and keeps the field ready for the next one, Enter in an empty field gives the keyboard
back. Why the island works this way is in [ADR 0003](adr/0003-keyboard-on-demand.md).

## System load

With the `system` permission, `system.stats()` tells how busy the computer is:

```js
import { system } from '/__sdk/index.js';

const stats = await system.stats();
// stats.cpu.usage (0–100), stats.cpu.cores (each logical processor), stats.cpu.name
// stats.memory.totalBytes, stats.memory.usedBytes
// stats.gpu?.usage (gpu is null when it cannot be measured)
// stats.battery?.percent, .charging, .pluggedIn, .secondsLeft (battery is null without one)
// stats.disks: [{ mount: 'C:', label: 'Windows', totalBytes, freeBytes }]
// stats.network.downBytesPerSec, stats.network.upBytesPerSec
// stats.uptimeSec
```

Processor and graphics usage are what Task Manager shows: the processor's "% Processor Utility"
and the busiest graphics engine summed over processes. Network rates count the computer's
hardware adapters only, so traffic through a VPN is not counted twice. Usages and rates are
measured since the previous sample, and one sample serves all widgets for half a second: poll
every one to a few seconds, and while the island is closed only if the live activity shows a
value. Temperatures and fan speeds need administrator rights on Windows, so they are not part
of it.

## Links and the clipboard

`links.open(url)` (permission `links`) opens an `http` or `https` link in the default browser.
`clipboard.writeText(text)` (permission `clipboard:write`) puts up to 1 MB of text on the
clipboard. Both act outside the widget, so they work only from a click: while the island is open
and the pointer is on the widget, like taking the keyboard. Otherwise, or more often than twice
a second, they reject with a `LizunError`.

```js
import { clipboard, links } from '/__sdk/index.js';

ipButton.addEventListener('click', () => clipboard.writeText(ip));
headline.addEventListener('click', () => links.open(article.url));
```

Why they work this way is in [ADR 0012](adr/0012-widget-capabilities.md).

## Sandbox

The iframe has an opaque origin, so a widget cannot read the island page, cookies or
`localStorage`, or call the Tauri backend. Scripts may only come from the widget's own folder
and `/__sdk/`; inline scripts are blocked. Network requests, by `fetch` or `network.request`,
are limited to `network`.

Both limits are a Content Security Policy that Lizun sends with every file of the widget, not
only its pages, so a worker started from the widget's folder is held to them too. SVG files
are served with scripts turned off. The CSP limits what the widget can load and `fetch`; it is
best-effort against side channels that browsers do not put under CSP, such as DNS prefetching
or WebRTC, so `network` tells the user where the widget talks to, not a guarantee that no data
can leave.

The widget stays on its `entry` page. Its frame cannot be pointed at anything but widget files,
and when a page in the frame navigates it anyway (to another file, or with
`location.reload()`), the island stops answering that frame and starts the widget over from
`entry` in a new one. A widget that keeps doing this right after every start is stopped. Build
a widget as a single page and change what it shows with scripts.

## Installing

In Settings > Widgets > Add a widget:

- paste a GitHub link and press Install;
- or choose a `.zip` archive or a widget folder on this computer, or drop one on the settings
  window.

Lizun downloads or copies the widget into a staging folder and checks it there: the manifest
must pass the rules above and the `entry` page must exist. A widget that fails shows the reason
and is not installed. One that passes is shown with its author, origin, permissions and network
access, and is installed only after the user presses Install. Why it works this way is in
[ADR 0005](adr/0005-installing-widgets.md).

The widget is put in `%APPDATA%\app.lizun.desktop\widgets\<id>\` and appears in the island
right away. Installing a widget whose id is already installed replaces it (the button says
Update); its settings and stored data are kept. The trash button next to an installed widget
removes it, also keeping its settings and data for a later reinstall.

Lizun remembers where each widget was installed from (in `widget-sources.json` next to the
`widgets` folder): the GitHub repository, whatever the branch, release or folder in the link,
or "this computer" for an archive or a folder. When a widget comes from somewhere else than the
widget Lizun had with that id, installed or removed, it may be by another author, so the
confirmation says so, and installing it deletes the settings and data saved for the earlier widget. The
same happens when Lizun does not know the earlier widget's source, for example because it was
copied in by hand.

Copying a widget folder into `%APPDATA%\app.lizun.desktop\widgets\` by hand and pressing Reload
works too, without the checks before installing. In development builds, widgets in the
repository's `widgets/` folder are loaded as well.

Limits: an archive up to 50 MB, a widget up to 50 MB and 2000 files once unpacked. Symbolic
links and `.git` folders are skipped.

## Publishing on GitHub

Put the widget in a public GitHub repository. These links install it:

| Link | What is installed |
|---|---|
| `https://github.com/you/clock` | The default branch. |
| `https://github.com/you/clock/tree/v1.2.0` | A branch, tag or commit. |
| `https://github.com/you/widgets/tree/main/clock` | The `clock` folder, for a repository with several widgets. |
| `https://github.com/you/clock/releases/tag/v1.2.0` | The source of a release. |
| `https://github.com/you/clock/releases/download/v1.2.0/clock.zip` | A zip file attached to a release. |
| `https://github.com/you/clock/releases/latest/download/clock.zip` | That file from the latest release. |

Lizun takes the folder with the `manifest.json` closest to the top of the archive, so the
manifest can sit at the root of the repository or in a single folder such as `dist/`. When
several folders at that level have one, it asks for a link to the folder instead. A `/tree/`
link reads the first segment after `tree` as the branch, so for a branch with a slash in its
name (`feature/x`) publish a tag or a release instead.

A link to the default branch always installs what is on it right now. To let people install a
fixed version, publish releases and share the release link, and raise `version` in the
manifest with each one: Settings shows it, and an update shows which version it replaces.

Widgets from private repositories cannot be installed.

To let others find your widget, add it to the [widget catalog](widget-catalog.md).

Widgets that ship with Lizun live in `apps/desktop/bundled-widgets/` and are copied into the
app's resources. They use the same SDK and sandbox as installed widgets and are listed with the
built-in ones in Settings. An installed widget cannot take the id of a bundled one.
