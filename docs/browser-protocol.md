# Browser media protocol

Some browsers, Yandex Browser among them, do not publish their media to the Windows media
controls, so Lizun cannot see it through the system. The Lizun browser extension
(separate repository, `lizun-browser-extension`) watches media in every tab and reports it to
the app over a WebSocket. This document is the contract between the two.

## Connection

- The app listens on `ws://127.0.0.1:<port>`; the port is a preference (Settings → Media,
  default **6869**) and the extension's popup must use the same one.
- The extension connects and reconnects (1 s doubling to 10 s) for as long as it runs.
- The app accepts only `chrome-extension://`, `moz-extension://` and `safari-web-extension://`
  origins, so a web page cannot connect. Clients without an `Origin` header are local programs
  and are let in.
- If the port is taken, the app keeps running, shows the error in Settings and retries every
  5 s.

Every message is one JSON text frame with a `type`. Unknown types and fields are ignored, so
either side can add messages without breaking the other.

## Extension → app

| type | fields | meaning |
| --- | --- | --- |
| `hello` | `protocol`, `version` | First message. `protocol` is `1`. |
| `player` | `id`, `player` | A frame has media, or its media changed. Full state every time. |
| `artwork` | `id`, `url`, `data` | The cover named by `player.artworkUrl`, as a `data:image/...` URL. |
| `removed` | `id` | The frame no longer has media (stopped, navigated, closed). |
| `ping` | | Sent every 20 s; keeps Chromium's service worker alive. |

Nothing but `hello` is sent until the app has answered with its own `hello`. After that the
extension sends every current player and cover.

`id` is `<tab id>:<frame id>`. A `player` is:

```json
{
  "site": "music.yandex.ru",
  "title": "Spit Shine",
  "artist": "Xzibit",
  "album": "8 Mile",
  "artworkUrl": "https://avatars.yandex.net/.../400x400",
  "playing": true,
  "position": 0.111,
  "duration": 219.13,
  "playbackRate": 1,
  "sampledAt": 1791390536463,
  "actions": ["play", "pause", "previous", "next", "seek"]
}
```

- `position` and `duration` are seconds and `null` without a timeline (live streams).
  `sampledAt` is the Unix time in milliseconds at which `position` was read; the position moves
  on from there while `playing`, so the extension does not resend it as it advances.
- `actions` lists what the page can do on request.
- A cover arrives after the `player` that names it, and only if it could be downloaded. A cover
  whose `url` no longer matches the player's `artworkUrl` is stale and ignored.

## App → extension

| type | fields | meaning |
| --- | --- | --- |
| `hello` | `protocol`, `app`, `version` | Answer to the extension's `hello`. |
| `command` | `id`, `action`, `position`? | Run `action` (`play`, `pause`, `previous`, `next`, `seek`) in that frame; `seek` takes `position` in seconds. |

The result of a command shows up as the next `player` message.

## How the extension reads media

The extension does not read any site's markup. A script in the page's own world runs before the
page's scripts and records what the page gives the browser's media controls:
`navigator.mediaSession` metadata, its action handlers, playback and position state, and the
`<audio>`/`<video>` elements, including ones never added to the document. Commands call the
page's own action handlers, the same thing a media key does. Muted elements and sounds shorter
than 10 s do not count as media.
