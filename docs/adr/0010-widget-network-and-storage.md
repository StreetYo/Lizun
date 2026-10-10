# ADR 0010: Network requests through the core, local apps, and storage in files

Status: accepted (2026-10-10). How widgets use it is described in
[widgets.md](../widgets.md#network) and [widgets.md](../widgets.md#storage).

## Context

The first widget that needed this is Anki: it reviews flash cards either from Anki on the same
computer or from decks it keeps itself. Anki is reached through the AnkiConnect add-on, an HTTP
API on `http://127.0.0.1:8765`. Three things stood in the way:

- A manifest's `network` list took only `https://` origins, and apps on the user's computer
  rarely serve HTTPS.
- A widget's own `fetch` is subject to CORS, and its iframe has an opaque origin, so it sends
  `Origin: null`. AnkiConnect, like many local and public APIs, answers only origins on its
  allow list; adding `null` there would let any sandboxed frame on any website reach the user's
  collection.
- `storage` kept values in the island's `localStorage`: 64 KB per value and a few megabytes
  for all widgets together, too little for decks of thousands of cards.

## Decision

- **`network` may list `http://127.0.0.1:port` and `http://localhost:port`.** The port is
  required, so a widget names one program and never gets the whole computer. Ports where what
  listens can take over the computer or Lizun are refused: 2375 and 2376 (Docker), 9222 and 9229
  (Chromium, WebView2 and Node debuggers), 6869 and whatever port the user moved Lizun's browser
  extension server to (checked when the request is sent). Plain HTTP to anything else is still
  refused. Settings and the install review show such an origin apart from the others, in bold,
  as "Talks to a program on this computer (port 8765)".
- **`network.request` in the SDK sends a request from the core** (`src-tauri/src/widget_network.rs`),
  with no CORS and no `Origin` header, so a widget reaches APIs made for apps, not web pages.
  The core checks every request against the widget's `network` list by scheme, host and port
  (an origin without a port means the scheme's default port; `*.example.org` matches
  subdomains only), does not follow redirects, so a redirect cannot lead off the list, refuses
  credentials in the URL and the `Host`, `Origin`, `Content-Length`, `Connection` and
  `Transfer-Encoding` headers, and limits bodies to 16 MB out, 32 MB back, one minute per
  request and four requests in flight per widget. Bodies are text. The widget's own `fetch`
  keeps working under the CSP built from the same list.
- **`storage` keeps one file per key** in `%APPDATA%\app.lizun.desktop\widget-data\<widget id>\`
  (`src-tauri/src/widget_storage.rs`): up to 1 MB of JSON per value and 50 MB per widget. A
  value is written to a temporary file and renamed into place. The file name is the key's UTF-8
  bytes in hex, so any key is a valid name. `storage.remove` deletes a key. Values an older
  Lizun kept in `localStorage` (`lizun.widget.<id>.<key>`) move to files before the widget's
  first storage call in a session. Ids contain dots, so the key is the part after the last dot
  and the id everything before it; the widgets of those versions never used dots in keys, and a
  name with a dot in the key part stays where it is. That way a widget whose id is a prefix of
  another's (`lizun` and `lizun.todo`) cannot take the other's values. Removing a widget keeps
  its data, like its settings.

## Consequences

- The Anki widget talks to AnkiConnect without the user editing AnkiConnect's config, and the
  add-on's protection against websites stays as it is.
- Any widget can now reach an API that does not allow browsers, but only on the origins the
  user saw and accepted at install time.
- A widget on `http://127.0.0.1:port` can drive whatever listens on that port, without the
  `Origin` check some local servers rely on. That is what the user accepts by installing it, and
  the install review says so plainly; the most dangerous ports are refused outright.
- Storage survives clearing the WebView's data and no longer competes with the island's own
  `localStorage`.

## Not done

- Binary request and response bodies; text covers JSON APIs, and base64 covers the rest.
- A widget talking to AnkiWeb directly: AnkiWeb serves only the official Anki clients, so the
  Anki widget syncs through Anki on the computer, and decks kept in the widget do not sync.
