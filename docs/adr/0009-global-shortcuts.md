# ADR 0009: Global shortcuts

Status: accepted (2026-10-10).

## Context

The island opens when the pointer rests on the top edge, and the music is controlled with the
pointer. People want to open the island and play, pause or skip a track from the keyboard or a
side mouse button, without reaching for the edge. A global shortcut takes its keys away from every
app, so a careless one breaks typing or Windows itself; the settings must not let that happen.

## Decision

- **Four actions**: open or close the island (Alt+Shift+Space by default), play or pause, next
  track, previous track (not set by default: keyboards have media keys). Each is set or turned
  off in Settings > Shortcuts and kept in `preferences.json` (`hotkeys`), written like
  `Ctrl+Alt+Shift+KeyK`: modifiers in a fixed order, then the key by its `KeyboardEvent.code`
  name, or `MouseBack` / `MouseForward`.
- **What a shortcut may be** is decided in the core (`check` in `src-tauri/src/hotkeys.rs`);
  the settings ask it while recording and show why a shortcut is refused, and the preferences
  drop a refused one, so a hand-edited file cannot get past it either:
  - the side mouse buttons, alone or with modifiers; the left, right and middle buttons and the
    wheel never;
  - F13–F24, which only macro keypads and mouse software send, alone or with modifiers;
  - any other key needs two modifiers, one of them Ctrl, Alt or Win: alone or with Shift a key
    types, and one modifier (Ctrl+C, Alt+F4, Win+D) belongs to apps and Windows;
  - Escape, Tab, Enter, Backspace, Delete, the lock keys, Print Screen, Pause, the menu key and
    the media keys never;
  - Ctrl+Alt (without Win) with a letter, digit or sign is refused: it is AltGr and types
    characters on many layouts;
  - the numpad digits and point never: with Num Lock off (or Shift held) they send Insert, End,
    the arrows and Delete, which `RegisterHotKey` cannot tell from those keys; the numpad signs
    are fine;
  - a list of combinations Windows, graphics drivers and Remote Desktop keep is refused (Win+Shift+S,
    virtual desktops, Ctrl+Alt+arrows, Ctrl+Alt+End, Win+Ctrl+Shift+B, the Copilot and Office
    keys…), and Ctrl+Shift+T/N/Z/V, which nearly every app uses;
  - two actions cannot share a shortcut.
- **Keyboard shortcuts go through `RegisterHotKey`** on a thread of their own
  (`src-tauri/src/platform/win/hotkeys.rs`). Windows only tells Lizun about those combinations
  and refuses one another app already has; the settings then show the shortcut as taken.
  Keys that type a character are named by position and turned into virtual keys through the
  keyboard layout, so a shortcut stays on the same physical key.
- **Side mouse buttons go through a low-level mouse hook**, installed only while a shortcut
  uses one. It only looks at side button presses with exactly the shortcut's modifiers, holds
  back that press and its release (so the browser does not also go back), and passes every other
  mouse event on untouched.
- **Recording**: a click on the shortcut field records the next press; set shortcuts are off
  meanwhile so they are recorded instead of firing. Esc, a click elsewhere or leaving the window
  cancels.
- **The island shortcut toggles it.** Opened that way the island stays while the pointer is on
  it and closes as usual when it leaves; if the pointer never comes, it closes after five
  seconds, so it does not keep covering the top of the screen. The island never takes focus.
- **Music shortcuts control the player shown on the island**, the same as its buttons.

## Alternatives considered

| Option | Why not |
|---|---|
| `tauri-plugin-global-shortcut` | Keyboard only, so side mouse buttons would need a hook anyway, and two code paths for one setting. |
| A low-level keyboard hook for everything | Sees every keystroke in every app, which security tools rightly flag; `RegisterHotKey` also reports conflicts. |
| Recording shortcuts in the core with the hooks | The settings window already receives the keys while it has focus; pausing the registered shortcuts is enough. |
