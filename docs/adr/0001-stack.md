# ADR 0001: Tauri 2 + Rust core, web UI, sandboxed web widgets

Status: accepted for the prototype (2026-10-07). Windows first, macOS later.

## Context

Lizun is an open-source dynamic island for Windows and macOS. Two hard requirements shape the
stack:

1. Users must be able to write, add and install their own widgets.
2. A polished look and feel: spring morphing, content blur-in, live activities in the
   collapsed island.

The app is always on screen, so idle CPU, battery and memory matter (high resource use is the
most common complaint about apps of this kind).

## Decision

- **Tauri 2** as the application framework, with the core in **Rust**: window management,
  media sessions, file tray, widget registry and protocol. OS-specific code lives in
  `src-tauri/src/platform/<os>` behind the same functions, so a platform is added as one module.
- **Island UI in TypeScript + Svelte 5**, rendered by the system WebView (WebView2 on Windows,
  WKWebView on macOS). It can be developed and screenshot-tested in a normal browser.
- **Widgets are web packages** (`manifest.json` + HTML/JS/CSS) running in sandboxed iframes with
  an opaque origin. They reach the system only through the SDK (postMessage); every call is
  checked against the manifest permissions, and network access is limited by a CSP built from
  the manifest. Built-in widgets will move onto the same SDK.
- **Click-through by window region** on Windows (`SetWindowRgn` to the island's bounds) instead
  of cursor polling.

## Alternatives considered

| Option | Third-party widgets | Animation | Resources | Why not |
|---|---|---|---|---|
| Electron | web, best isolation | 120 Hz everywhere | 200+ MB RAM, ~100 MB installer | heavy for an always-on utility; kept as plan B (UI and SDK port 1:1) |
| Qt 6 / QML | runtime QML, no sandbox | excellent | light | widgets would be arbitrary unsandboxed code; small author pool |
| Avalonia | .NET plugins, full trust | good | medium | no click-through API, NSPanel by hand, no sandbox |
| Flutter | no dynamic code (AOT) | excellent | medium | widgets would need a second runtime |
| Native per OS (SwiftUI + WinUI) | WebView-hosted only | best | lowest | every screen built twice |

Every plugin system we found that is cross-platform by design (Zebar, Raycast 2.0, Figma,
Stream Deck) uses JS/HTML; every native-plugin system is tied to one OS and unsandboxed.

## Consequences and known risks

- **macOS animation and battery.** WKWebView caps JS-driven animation at 60 Hz on ProMotion
  displays ([WebKit 294338](https://bugs.webkit.org/show_bug.cgi?id=294338), open), and a
  transparent Tauri window on macOS 26 recomposites continuously, about 8x idle GPU power
  ([tauri#15471](https://github.com/tauri-apps/tauri/issues/15471)). Expect the collapsed island
  and its morph to be drawn natively on macOS (Core Animation in an NSPanel), with the WebView
  only inside the open panel. Two other Tauri island projects (EchoIsland, openNook) moved the
  island to native rendering.
- **Windows memory** is about the same as Electron, since WebView2 is Chromium.
- Tauri injects its IPC object into iframes. Calls from a sandboxed widget iframe are never
  answered or executed (verified), but this relies on Tauri internals; keep checking on upgrades.
- Fonts and icons: SF Pro and SF Symbols are licensed for Apple platforms only, so the UI uses
  Inter and Phosphor icons on every OS. No third-party names, artwork or assets are used.
