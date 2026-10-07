# ADR 0004: Widgets describe live activities, the island draws them

Status: accepted (2026-10-08). The format is described in [widgets.md](../widgets.md#live-activity).

## Context

The closed island showed only the current track and the number of files in the tray. With
neither, it was an empty pill. Widgets could not use it, although many have something worth a
glance while the island is closed: tasks left, a timer, the next meeting.

A widget runs in a sandboxed iframe that lives in the open panel (ADR 0001). The mini island is
32 px high, always on top of every app, and part of Lizun's own look.

## Decision

- **Declarative content.** A widget calls `activity.set({ leading, title, trailing })`: each end
  an icon from a fixed set, a text of up to 16 characters and an optional color, and a line of
  text in the middle. The island checks it and draws it like its own activities.
  `activity.clear()` removes it.
- **A capsule, not ears.** Windows has no camera notch, so the mini island is as wide as its
  content (within limits) and the middle carries the most useful part: the track title, the
  next task. Spreading the ends around an empty middle, as a notch forces on a MacBook, looked
  broken on a flat screen. On a notched screen the title will be dropped and the ends placed
  beside the notch, so widgets keep the ends meaningful on their own.
- **Behind a permission** (`activity`), shown in Settings like the others: the content appears
  on top of every app, so the user should see which widgets can put it there.
- **One activity at a time**, by a fixed order: the current track, then files in the tray, then
  widgets in their order in the island. Reordering widgets is how the user sets priority, and
  hiding a widget hides its activity.
- **Nothing to show, no mini island.** The pill goes into the top edge, as in the "Hidden" mode,
  and the edge still opens the island.
- **Kept by the island**, by widget id, while the widget's frame reloads, so the mini island does
  not flicker; a widget sets or clears it when it starts.
- In "Hide after a delay" mode, an activity coming up brings the mini island out; updates to it
  do not.

## Alternatives considered

| Option | Why not |
|---|---|
| A second, small iframe of the widget in the mini island | Arbitrary content at the top of the screen can imitate system UI; a frame per widget costs memory and startup time for 30 px of content; the ears would look different for every widget. |
| Widgets send an image | Same look problem, and text in an image is blurry at the island's scale and cannot follow the theme. |
| Priority declared by each widget (`priority: high`) | Every widget would claim the highest one; the user already orders widgets, which says what matters to them. |
| Rotating through all activities | Moving content at the edge of vision is distracting; can come later as an option. |

## Consequences

- Widgets are limited to the icon set and short text until the format grows (a progress ring and
  a small image are the likely next additions).
- Third-party widgets with the new permission are rejected by older Lizun builds, like the
  `settings` field (ADR 0002).
- The "Always visible" mode now means "visible while there is something to show".
