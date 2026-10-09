# Windows spike: results

Date: 2026-10-07. Machine: Windows 11 Pro 25H2 (26200), primary display 3200x2000 at 200%,
secondary 2560x1440 at 100%, WebView2 154. Tauri 2.12.1, Svelte 5.57, debug and release builds.

All checks below were run against the real app on the desktop: window state was read with
Win32 APIs (styles, window region, `WindowFromPoint`, foreground window), the UI was driven
through the WebView2 DevTools protocol, and pointer input was injected with `SetCursorPos` /
`mouse_event`. Files were dragged with a real OLE `DoDragDrop` from a helper window.

## Go/no-go checks

| # | Check | Result |
|---|---|---|
| 1 | Overlay window: topmost, not in Alt+Tab or the taskbar, never takes focus | **Pass.** Styles `TOPMOST TOOLWINDOW NOACTIVATE`; the foreground window stays the user's app at startup, on hover and after clicks inside the island. |
| 2 | Hover opens with a spring and content blur-in, closes on leave | **Pass.** Real cursor: opens ~90 ms after entering, closes ~320 ms after leaving. Frames: 16.7 ms average, p95 16.8 ms on a 60 Hz display, occasionally one late frame at the start. |
| 3 | Clicks outside the island reach the windows below | **Pass.** `SetWindowRgn` limits the window to the island bounds; `WindowFromPoint` returns the island inside and the window below outside, collapsed and open. No cursor polling. |
| 4 | Clicks inside the island work | **Pass.** Real clicks reach the page (tabs switch) without activating the window. |
| 5 | Drag files from Explorer onto the island | **Pass.** The island switches to the drop zones, a drop on "Files Tray" adds the files; image previews load through the asset protocol. |
| 6 | Drag files out of the tray into another app | **Pass.** The target receives a normal file drop (`FileDrop`, `Shell IDList Array`); the app stays responsive. |
| 7 | Third-party widget in a sandbox | **Pass.** The example widget gets the theme, now playing and storage. Blocked: island DOM (SecurityError), Tauri IPC (no answer, command not executed), network (CSP), `localStorage`, SDK calls without permission (`permission_denied`). |
| 8 | Now playing from SMTC | **Pass.** Title, artist and cover from a Telegram media session. |
| 9 | Idle resources | See below. |

## Resources

Release build (`lizun.exe` is 5.1 MB including the UI). Measured over the app process and all
its WebView2 child processes, 20–30 s per state, with the media session paused (nothing
animating). "Memory" is the private working set, the value Task Manager shows.

| State | Memory | CPU | GPU |
|---|---|---|---|
| Collapsed, after start | 154 MB | 0.44% of one core | 0% |
| Open, cursor resting on the panel | 210 MB | 0.15% of one core | 0% |
| Collapsed again after opening | 257 MB | 0.22% of one core | 0% |

- 8 processes: `lizun.exe` (~45 MB working set) plus the WebView2 browser, GPU, utility and two
  renderer processes. The second renderer is the widget iframe (site isolation); all widgets
  are served from one host, so they share it.
- CPU stays near zero because nothing polls: media updates are SMTC events and hover comes
  from the window region, not cursor polling. No continuous GPU work while idle.
- Memory is dominated by WebView2, as expected for this stack, and Chromium keeps what it used
  after the first open. Next step: lower the WebView2 memory target
  (`MemoryUsageTargetLevel = Low`) while collapsed and measure again.

## Problems found and fixed

- **Cover art from Telegram did not show.** SMTC reported the content type as
  `image/jpeg,image/jpe,image/jpg`, which made an invalid data URL. The type is now sniffed
  from the image bytes.
- **File drops were ignored.** wry attaches its drop handler to the WebView's child windows
  when the WebView is created, and those windows only exist once it is visible. The island
  window is now created visible (transparent, at the primary monitor's origin) with a small
  initial hit region set from Rust.
- **Switching to the Tray tab closed the island.** The tray was narrower, so the island
  shrank out from under the cursor. The tray now takes the width of the widget row.
- **Dragging a file out started the WebView's own image drag**, and our drag icon failed on a
  tainted canvas. Native dragging is disabled on tray items and the icon is drawn from a
  fetched blob.
- **Widget handshake failed** because a Svelte reactive proxy cannot go through postMessage;
  the manifest is passed as a plain snapshot.

## Open issues

- Fullscreen apps: the island stays on top of borderless fullscreen windows; it should hide or
  only appear on hover (detect with `SHQueryUserNotificationState`).
- Multiple monitors and DPI changes at runtime: the island is placed once on the primary
  monitor; it does not follow monitor or resolution changes yet.
- Keyboard input (notes, tasks) needs the window to become focusable temporarily.
- Share zone (Windows share sheet / Nearby Share) is a placeholder.
- Volume and brightness HUD replacement, timers, notes, tasks: not started. Calendar events come from an
  iCal link (ADR 0006).
- Windows 11 has a system "Drag tray" that can appear at the top of the screen during file
  drags; check how it interacts with the island on machines where it is enabled.
