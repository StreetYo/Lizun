# Visual reference

Measured from a reference screen recording (1920x1060, 30 fps; kept locally, not in the
repository) by tracking the black shape frame by frame. Values are in video pixels; the app
uses logical pixels with the same proportions.

## Shape

| State | Size | Corners |
|---|---|---|
| Collapsed with live activity | ~358 x 36 | flush with the top edge, rounded bottom |
| Open (Nook tab) | full width x ~260 | large bottom radius |
| Drag over (Files Tray + AirDrop zones) | ~810 x ~200 | large bottom radius, soft shadow |

The Tray tab keeps the width of the Nook tab.

## Motion

- **Open:** underdamped spring. Reaches the target in ~0.3 s, overshoots ~7% (height 260 → 276),
  settles by ~0.75 s. Roughly SwiftUI `.spring(response: 0.5, dampingFraction: 0.65)`, or
  stiffness ≈ 165, damping ≈ 17 at mass 1.
- **Close:** ~0.3 s, no visible bounce (critically damped).
- **Content:** blurred and faded while the shape grows, sharp ~0.5 s after the start; on close it
  blurs out immediately.
- Hover opens after ~0.1 s.

## Colors

| Token | Value |
|---|---|
| Island background | `#000000` |
| Cards, active tab, mirror circle | `#171619`–`#1d1c1f` (grey with a slight blue tint) |
| Accent (selected day, notes color, play button) | `#0084fc` |
| AirDrop / Share zone | `#072038` |
| Progress bar fill | `#eaeaea` |

## Layout notes

- Header: `Nook` / `Tray` pill tabs on the left, settings gear on the right.
- Widgets in one row separated by thin vertical dividers: media (cover, title, artist,
  controls, progress with times), mirror (circle), calendar (month, week strip with today in
  accent, events), notes, timers, tasks.
- Tray: dashed rounded area, file thumbnails with a white border, names with a middle ellipsis.
