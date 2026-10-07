# ADR 0003: The island takes the keyboard only while a text field is in use

Status: accepted (2026-10-07).

## Context

The island is a tool window that never activates (`WS_EX_NOACTIVATE`, and `MA_NOACTIVATE` on
clicks), so pressing a button in it leaves the user's app active and typing keeps going there.
This is a core promise of Lizun (ADR 0001, README).

A non-active window gets no keyboard input, so no widget could have a text field. The To-do
widget needs one, and so will others (search, notes, a quick calculator).

## Decision

- The island keeps never taking focus, except while a text field in a widget is in use.
- The SDK detects it on its own: pressing or focusing an editable `<input>`, a `<textarea>` or a
  `contenteditable` element asks the island for the keyboard (`keyboard.set`, no permission).
  Widget authors write ordinary HTML.
- The core (`island_keyboard`) remembers the foreground window, clears `WS_EX_NOACTIVATE` and
  brings the island to the foreground, falling back to Tauri's synthetic Alt press when Windows
  refuses the change (the click went to the WebView2 process, not to Lizun's).
- The keyboard is given back when the field loses focus, on Esc, on a click on anything that is
  not a text field (in a widget or on the island itself), when the island closes, through
  `keyboard.release()`, or when another window becomes active. The last case is caught in the
  window procedure (`WM_ACTIVATE`): the island page cannot see it, because its own `blur`
  already fired when the focus moved into the widget's iframe. Giving it back restores `WS_EX_NOACTIVATE` and re-activates the
  remembered window, but only if the island is still in the foreground, so a click on another
  app is never undone.
- While the keyboard is held the island stays open when the cursor leaves it.

## Alternatives considered

| Option | Why not |
|---|---|
| Typing only in separate windows (settings window, a popup) | Adding a task becomes a detour through another window; every widget with input would need its own flow. |
| A keyboard hook that forwards keystrokes without activating the island | Needs a global low-level hook, looks like a keylogger to security tools, and IME input would not work. |
| Making the island an ordinary activatable window | Breaks the promise: every click on a button would take the focus away from the user's app. |

## Consequences

- The synthetic Alt press may briefly highlight the menu bar of the previous app in rare cases.
- The window style is changed behind Tauri's back (as `prepare_island_window` already does), so
  Tauri upgrades that rewrite extended styles need checking.
- macOS will need the same behaviour from its native panel (`NSPanel` that becomes key only
  while a field is edited).
