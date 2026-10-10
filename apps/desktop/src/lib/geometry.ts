/** Island dimensions in logical pixels. */

/** Width of the strip along the top edge that opens the island while the mini island is hidden. */
export const PILL_WIDTH = 200;
/** The mini island is a capsule as wide as what it shows, within these bounds. */
export const CAPSULE_MIN_WIDTH = 88;
export const CAPSULE_MAX_WIDTH = 360;
export const COLLAPSED_HEIGHT = 32;
export const COLLAPSED_RADIUS = 12;
/** Going into the top edge, the mini island narrows to this share of its width as it rises. */
export const CONCEALED_WIDTH_SCALE = 0.6;
export const EXPANDED_RADIUS = 30;

/** Size of the island while files are dragged over it. */
export const DROP_SIZE = { width: 520, height: 150 };

/** Room around the expanded island for its shadow; it stays inside the input region. */
export const SHADOW_MARGIN = { side: 32, bottom: 44 };
/** Extra hover area around the collapsed pill so it is easy to hit. */
export const HOVER_MARGIN = 4;

/** Delay before leaving closes the island. Opening waits for the user's hover delay preference. */
export const CLOSE_DELAY_MS = 320;

/** The island opened by its shortcut closes after this long if the pointer never comes to it. */
export const SHORTCUT_CLOSE_DELAY_MS = 5000;

/** While the mini island is hidden, only this strip along the top edge reacts to the cursor,
 * so the invisible island does not block clicks on the windows below. */
export const HOT_ZONE_HEIGHT = 4;

export interface IslandSize {
  width: number;
  height: number;
  radius: number;
}
