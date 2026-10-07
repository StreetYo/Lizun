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
| `permissions` | `media:read`, `media:control`, `storage`. Anything else is rejected. |
| `network` | Origins the widget may `fetch` (`https://host[:port]`, `*.` allowed). Empty means no network. |

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

## Sandbox

The iframe has an opaque origin, so a widget cannot read the island page, cookies or
`localStorage`, or call the Tauri backend. Scripts may only come from the widget's own folder
and `/__sdk/`; inline scripts are blocked. Network requests are limited to `network`.

## Installing

Put the folder in `%APPDATA%\app.lizun.desktop\widgets\` and reopen the island. In development
builds, widgets in the repository's `widgets/` folder are loaded too.
