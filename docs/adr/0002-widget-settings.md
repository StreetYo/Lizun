# ADR 0002: Widget settings declared in the manifest

Status: accepted (2026-10-07). Implemented for installed widgets; the built-in widgets follow
when they get options. The format is described in [widgets.md](../widgets.md#settings).

## Context

Widgets can be installed and removed, and many need options the user picks: a city for a
weather widget, the first day of the week for the calendar, which camera the mirror uses. Until
now Lizun had only app-wide settings (island visibility, delays, launch at login).

Settings have to fit the widget model from ADR 0001:

- A widget runs in a sandboxed iframe with an opaque origin and reaches the system only through
  the SDK. It cannot open windows or call the Tauri backend.
- The settings window is a separate window, and a widget is loaded only while the island is
  open.
- A broken or hostile widget must not be able to corrupt Lizun's stored data or show a settings
  screen that looks like part of Lizun.

## Decision

- **Two levels.** Options of the island itself (visibility, auto-hide, hover delay, showing the
  mini island on music changes, launch at login) stay app preferences in Settings > General.
  Options of one widget belong to that widget and are listed under it in Settings > Widgets.
- **Declarative schema.** A widget lists its options in `manifest.json` under `settings`, using
  four types: `toggle`, `number`, `select`, `text`. Lizun draws the controls itself, in the same
  style as its own settings, and the widget ships no settings UI.
- **Lizun owns the values.** The Rust core validates the schema when it loads the manifest,
  stores the values per widget id in a JSON file in the app config folder (next to
  `preferences.json`), checks every write against the schema, and broadcasts changes to all
  windows. Values are fixed up against the current schema on load (clamped, reset to `default`,
  unknown keys dropped), so a widget update never sees values of the wrong type.
- **Read-only for widgets, no permission needed.** The values arrive in the `ready()` context and
  through `settings.subscribe()`. Only the user changes them; anything a widget writes itself
  stays in `storage`.
- **Built-in widgets use the same format**, declared in code, so all widgets share one settings
  screen and one store. This is a step towards moving built-in widgets onto the SDK.

## Alternatives considered

| Option | Why not |
|---|---|
| Each widget ships its own settings page (an HTML file Lizun opens in the settings window) | Every widget looks different; a widget page inside Lizun's window can imitate Lizun's own UI; values are opaque to Lizun, so it cannot validate or reset them. Kept as a possible addition for widgets that outgrow the schema. |
| Settings inside the widget's column in the island | The island is small and closes as soon as the cursor leaves; text input there is awkward, and a widget the user cannot open (it fails to load) cannot be configured. |
| Widgets keep options in `storage` and build their own controls | Same as the first row, plus options would be lost or hidden whenever a widget fails to load. |
| JSON Schema as the format | Far more than four control types need, harder to validate safely, and it does not say how a field is shown. |

## Consequences

- Widget authors get settings for free and cannot make them inconsistent, but are limited to
  four control types until the format grows.
- `WidgetManifest` uses `deny_unknown_fields`, so a widget that declares `settings` is rejected
  by Lizun builds that predate this feature. Widgets will need a way to state the minimum Lizun
  version before the format is published.
- The settings window has to load the widget list and schemas from the core (it already does for
  the list) and does not need the widget's code.
- Values of uninstalled widgets stay in the file, so reinstalling restores them. A cleanup (for
  example, dropping values of widgets missing for a long time) can come later.

## Later

- Enabling, disabling and reordering widgets: kept by Lizun per widget id, alongside these
  values, but not part of the manifest.
- `settings.open()` so a widget can offer a gear button that opens its section in Settings.
- Conditional fields (show one field only when a toggle is on), more types (color, time), and
  translated labels.
- If widgets get live activities in the mini island, the "show on changes" option would move
  from app preferences to each widget that provides one.
