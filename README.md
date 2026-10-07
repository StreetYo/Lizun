# Lizun

An open-source "dynamic island" for the desktop: a black island at the top of the screen that
opens on hover and holds widgets: now playing, a file tray, a calendar, a camera mirror, and
widgets anyone can write and install.

> Status: early prototype. Windows works; macOS is planned (see [ADR 0001](docs/adr/0001-stack.md)).

## What works today (Windows)

- Collapsed pill with live activities (cover art and equalizer for the current track, file count).
- Hover to open: spring animation with a content blur-in, closes when the cursor leaves.
- Never steals focus, stays out of Alt+Tab and the taskbar; clicks outside the island go to the
  windows below.
- Now playing for any app in the Windows media flyout (Spotify, browsers, Telegram, ...), with
  play/pause/next/previous.
- File tray: drag files from Explorer onto the island, drag them back out into any app.
- Third-party widgets in a sandbox, with a permission manifest (see [docs/widgets.md](docs/widgets.md)).
- Settings window (gear in the island or the tray menu): launch at login, installed widgets
  with their permissions, the widgets folder, quit.
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
