/** Island dimensions in logical pixels. */

/** Width of the idle pill. */
export const PILL_WIDTH = 200;
/** Extra width on each side while a live activity is shown in the collapsed island. */
export const EAR_WIDTH = 72;
export const COLLAPSED_HEIGHT = 32;
export const COLLAPSED_RADIUS = 12;
export const EXPANDED_RADIUS = 30;

/** Size of the island while files are dragged over it. */
export const DROP_SIZE = { width: 520, height: 150 };

/** Room around the expanded island for its shadow; it stays inside the input region. */
export const SHADOW_MARGIN = { side: 32, bottom: 44 };
/** Extra hover area around the collapsed pill so it is easy to hit. */
export const HOVER_MARGIN = 4;

/** Delay before hovering opens the island, and before leaving closes it. */
export const OPEN_DELAY_MS = 90;
export const CLOSE_DELAY_MS = 320;

export interface IslandSize {
  width: number;
  height: number;
  radius: number;
}
