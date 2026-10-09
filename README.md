# Lizun

An open-source "dynamic island" for the desktop: a black island at the top of the screen that
opens on hover and holds widgets: now playing, a file tray, a calendar, a camera mirror, a to-do
list, and widgets anyone can write and install.

> Status: early prototype. Windows works; macOS is planned (see [ADR 0001](docs/adr/0001-stack.md)).

## What works today (Windows)

- Mini island with live activities, a capsule as wide as what it shows: cover, title and
  equalizer for the current track, files in the tray, and activities of widgets (the To-do
  widget shows the next task). With nothing to show, it goes into the top edge.
- Hover to open: spring animation with a content blur-in, closes when the cursor leaves.
- Never steals focus, stays out of Alt+Tab and the taskbar; clicks outside the island go to the
  windows below. The only exception is typing into a widget's text field, after which the
  focus goes back to your app.
- To-do list, written on the widget SDK and shipped with the app.
- Calendar: the week around today with your events, from the iCal link of a Google, Outlook or
  iCloud calendar or from an .ics file (Settings > Widgets > Calendar), recurring events included.
- Now playing for any app in the Windows media flyout (Spotify, browsers, Telegram, ...), with
  play/pause/next/previous.
- File tray: drag files from Explorer onto the island, drag them back out into any app.
- Third-party widgets in a sandbox, with a permission manifest and their own settings (see
  [docs/widgets.md](docs/widgets.md)). Install them in Settings from a GitHub link, a zip
  archive or a folder: Lizun checks the widget and shows its permissions before installing it.
- Widgets can be dragged into another order and hidden, right in the island (pencil button) or
  in Settings > Widgets.
- Settings window (gear in the island or the tray menu): launch at login, how the closed
  island shows (always, hide after a delay, hidden) and whether music changes bring it out,
  how long to hover before it opens, installed widgets with their permissions and settings,
  the widgets folder, quit.
- Tray icon to open settings, hide the island or quit.

## Development

Requirements: Windows 10/11 with the WebView2 runtime, [Node.js](https://nodejs.org) 22+,
[pnpm](https://pnpm.io) 10, [Rust](https://rustup.rs) (stable, MSVC toolchain) and the
Visual Studio C++ build tools.

```bash
pnpm install
pnpm --filter @lizun/widget-sdk build
pnpm dev
```

`pnpm dev` starts Vite and the Tauri app with hot reload. The island UI can also be opened in a
plain browser at the Vite URL, where the backend is mocked (useful for design work).

## Layout

| Path | What it is |
|---|---|
| `apps/desktop/src` | Island UI (Svelte 5 + TypeScript) |
| `apps/desktop/src-tauri` | Rust core: window, hit region, media, widget protocol |
| `apps/desktop/src-tauri/src/platform` | OS-specific code (`win/` today; macOS goes next to it) |
| `packages/widget-sdk` | SDK that widgets import |
| `widgets/hello` | Example third-party widget |
| `docs/` | Decisions, specs and test reports |

## License

Not chosen yet.
