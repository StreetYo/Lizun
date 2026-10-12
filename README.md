# Lizun

An open-source "dynamic island" for the desktop: a black island at the top of the screen that
opens on hover and holds widgets: now playing, a file tray, a calendar, a camera mirror, a to-do
list, and widgets anyone can write and install.

![Lizun: the island opens on hover, a task is added in the To-do widget, widgets are rearranged, the clipboard history is shown and files are dropped into the tray](https://raw.githubusercontent.com/StreetYo/Lizun/master/docs/media/lizun-demo.gif)

> Status: early prototype. Windows works; macOS is planned (see [ADR 0001](docs/adr/0001-stack.md)).

## Download

Download Lizun for Windows 10/11 from the [latest release](https://github.com/StreetYo/Lizun/releases/latest):
`Lizun_*_x64-setup.exe` is the installer for most people; an MSI for managed installs and a
portable zip are there too. The files are not code-signed yet, so SmartScreen may warn about
them: click **More info**, then **Run anyway**.

## What works today (Windows)

- Mini island with live activities, a capsule as wide as what it shows: cover, title and
  equalizer for the current track, files in the tray, and activities of widgets (the To-do
  widget shows the next task). With nothing to show, a pair of eyes looks around and blinks
  in it, or, with the eyes turned off, it goes into the top edge.
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
- Clipboard history: text, pictures and files you copy, in the island's Clipboard tab; click
  one to copy it again. How long entries stay and how many are kept (or no limit) are set in
  Settings > Clipboard; passwords that apps mark as private are never saved.
- Global shortcuts to open or close the island (Alt+Shift+Space by default) and for play/pause,
  next and previous track, set or turned off in Settings > Shortcuts. Side mouse buttons work
  too; single keys, the main mouse buttons and combinations Windows needs are refused (see
  [ADR 0009](docs/adr/0009-global-shortcuts.md)).
- Third-party widgets in a sandbox, with a permission manifest and their own settings (see
  [docs/widgets.md](docs/widgets.md)). Install them in Settings from a GitHub link, a zip
  archive or a folder: Lizun checks the widget and shows its permissions before installing it.
  Widgets ready to install are listed in the [widget catalog](docs/widget-catalog.md).
- Widgets can be dragged into another order or a second row and hidden, right in the island
  (pencil button) or in Settings > Widgets.
- Settings window (gear in the island or the tray menu): language, launch at login, how the closed
  island shows (always, hide after a delay, hidden) and whether music changes bring it out,
  how long to hover before it opens, installed widgets with their permissions and settings,
  the widgets folder, quit.
- Tray icon to open settings, hide the island or quit.
- English and Russian, following the Windows display language or picked in Settings > General.
  Widgets get the language too and can translate their name and settings.

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

## Translating

Every text lives in `apps/desktop/src/locales/<code>.json`, one file per language, shared by
the UI and the Rust core. To add a language:

1. Copy `en.json` to `<code>.json` (a language tag such as `de` or `pt-BR`) and translate the
   values; keep the keys and the `{placeholders}`. Set `languageName` to the language's name in
   itself. Plural texts have one key per form (`_one`, `_few`, `_many`, `_other`, as
   [CLDR](https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html)
   lists them for the language).
2. Add the file to `SOURCES` in `apps/desktop/src-tauri/src/i18n.rs`.
3. Run `cargo test` in `apps/desktop/src-tauri`: it fails on missing texts and wrong
   placeholders.

Texts left out of a translation are shown in English. How it works: [ADR 0007](docs/adr/0007-languages.md).

## Layout

| Path | What it is |
|---|---|
| `apps/desktop/src` | Island UI (Svelte 5 + TypeScript) |
| `apps/desktop/src/locales` | Interface texts, one file per language |
| `apps/desktop/src-tauri` | Rust core: window, hit region, media, widget protocol |
| `apps/desktop/src-tauri/src/platform` | OS-specific code (`win/` today; macOS goes next to it) |
| `packages/widget-sdk` | SDK that widgets import |
| `widgets/hello` | Example third-party widget |
| `docs/` | Decisions, specs and test reports |

## License

Lizun is free software, licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option. You may use, copy, change, share and sell it, in open or closed projects.

Unless you explicitly state otherwise, any contribution you submit for inclusion in Lizun, as
defined in the Apache-2.0 license, is licensed as above, without any additional terms or
conditions.
