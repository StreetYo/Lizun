# ADR 0011: Widgets that start a program on this computer

Status: accepted (2026-10-10). How widgets use it is described in
[widgets.md](../widgets.md#starting-a-program).

## Context

The Anki widget shows cards from Anki through the AnkiConnect add-on, which answers only while
Anki runs. Having to open Anki first, wait for it, and keep its window around defeats the point
of reviewing from the island. A widget runs in a sandbox and cannot start programs, and it
should not get a general way to do so.

## Decision

- **The manifest lists the programs** a widget may start, under `apps`: an id, a name to show,
  and up to eight paths it may be installed at, tried in order. A path is an `.exe` under a fixed
  folder: `%LOCALAPPDATA%`, `%APPDATA%`, `%PROGRAMFILES%`, `%PROGRAMFILES(X86)%` or a drive
  letter, with no `..`, other variables or arguments. The widget names the program by id only.
  Settings and the install review show "Starts Anki" next to the other permissions.
- **`apps.start(id, { hidden })`** starts the first path that exists, without arguments and in
  its own folder, unless a process from that file is already running. It answers `started`,
  `running` or `notFound`.
- **In the background (`hidden`)**, the process is created suspended, a WinEvent hook for its
  windows is installed, and only then does it run: the first window obeys `SW_HIDE` from
  `STARTUPINFO` and every window it shows during the next 30 seconds is hidden at once, so it
  starts without a window or a taskbar button, without flashing. After that the hook stops, so
  opening the program by hand later shows it as usual (for Anki, its shortcut raises the running
  instance).
- **`apps.show(id)`** shows the program's main windows (top-level, no owner, title bar and
  window menu, a title, not a tool or non-activating window) and brings the first to the front.
  Toolkits keep hidden helper windows (Qt's title bar and screen observers, Chromium widgets)
  that must stay hidden, hence the strict test.
- **When Lizun quits**, a program it started in the background whose main windows are all
  still hidden is sent `WM_CLOSE`, as if its close button had been pressed. Anki then closes
  normally and syncs on the way out. One that was shown keeps running.
- **The widget decides when.** The Anki widget calls `apps.start` when it loads, which is when
  Lizun starts, and only if it is in the island and set to use Anki. A widget that is not
  installed, is hidden, or uses its own decks starts nothing. When the user closes Anki, the
  widget does not start it again until asked (Try again or Open Anki).

## Consequences

- Cards from Anki are ready when the island is first opened; Anki costs its usual memory
  (about 400 MB) in the background.
- A widget can start one of the few programs its manifest names, and the user sees which ones
  before installing. It cannot pass arguments, choose another path or start anything else.
- Windows only, like the rest of the platform layer; elsewhere `apps.start` fails.

## Not done

- Stopping a program while Lizun runs: closing it is the user's choice, in the program.
- A path the user picks: a widget setting cannot ask for files (ADR 0002), and the common
  install folders cover Anki.

## Revised 2026-10-19: back to the background, and the shortcut

Anki started in the background could not be opened from its own shortcut. Anki hands a second
start to the running instance and asks it to raise its window, but Qt does not learn of a window
hidden from outside and takes it for shown, so the raise did nothing (minimizing first does not
help either). Starting Anki minimized instead was tried and dropped: it keeps a taskbar button.

- **`apps.hide(id)`** puts a running program back in the background: its main windows and the
  windows they own (an open dialog) are hidden; `apps.show` brings back both. Both act outside
  the widget, so they work only from a click on the open widget. **`apps.state(id)`** answers
  `notRunning`, `hidden` or `shown`, so the widget's button can say which it will do.
- **While a program is in the background**, Lizun checks five times a second for another
  process from the same file and shows the program when one appears: that is its shortcut being
  used. Anki's second instance lives about half a second. Only processes with the program's file
  name are opened, so a check costs a process snapshot.
- **When Lizun quits**, a program it started in or put in the background is closed as before if
  it is still there.
