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
| `entry` | HTML file to load (default `index.html`). |
| `width` | Column width in logical pixels, 120–640 (default 240). |
| `permissions` | `media:read`, `media:control`, `storage`, `activity`. Anything else is rejected. |
| `network` | Origins the widget may `fetch` (`https://host[:port]`, `*.` allowed). Empty means no network. |
| `settings` | Options the user can change in Settings > Widgets, see [Settings](#settings). |

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
| `type` | `toggle`, `number`, `select` or `text`. |
| `label` | Shown next to the control, up to 60 characters. |
| `description` | Optional hint under the label, up to 200 characters. |
| `default` | Required. The value until the user changes it; it must be valid for the type. |

Types and their extra fields:

| Type | Value | Extra fields |
|---|---|---|
| `toggle` | `true` / `false` | none |
| `number` | number | `min` and `max` (required), `step` (default 1), `unit` (optional label such as `s` or `min`, up to 10 characters) |
| `select` | one of the option values | `options`: 2–20 `{ "value": string, "label": string }`; values are 1–64 characters and unique, labels up to 60 |
| `text` | string | `maxLength` (1–500, default 100), `placeholder` (optional, up to 100 characters) |

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
  then widget activities in the order the widgets have in the island. The user sets the
  priority by moving widgets, and a hidden widget shows nothing.
- When nothing has an activity, the mini island goes into the top edge; pointing at the edge
  still opens the island.
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
the field loses focus, when Esc is pressed in it, or when the user clicks anything else.
Nothing needs to be declared for this.

To hand the keyboard back yourself, for example after a form is submitted:

```js
import { keyboard } from '/__sdk/index.js';

keyboard.release();
```

The bundled To-do widget (`apps/desktop/bundled-widgets/todo`) is a complete example: Enter adds
a task and keeps the field ready for the next one, Enter in an empty field gives the keyboard
back. Why the island works this way is in [ADR 0003](adr/0003-keyboard-on-demand.md).

## Sandbox

The iframe has an opaque origin, so a widget cannot read the island page, cookies or
`localStorage`, or call the Tauri backend. Scripts may only come from the widget's own folder
and `/__sdk/`; inline scripts are blocked. Network requests are limited to `network`.

## Installing

Put the folder in `%APPDATA%\app.lizun.desktop\widgets\` and reopen the island. In development
builds, widgets in the repository's `widgets/` folder are loaded too.

Widgets that ship with Lizun live in `apps/desktop/bundled-widgets/` and are copied into the
app's resources. They use the same SDK and sandbox as installed widgets and are listed with the
built-in ones in Settings. An installed widget cannot take the id of a bundled one.
