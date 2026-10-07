/**
 * Live activities that widgets show in the closed island. A widget only describes what to show
 * (an icon or a short text at each end, a line of text in the middle); the island draws it, so
 * a widget cannot put arbitrary content at the top of the screen.
 */

/** Icons a live activity can use, by the name widgets pass in. */
export const ACTIVITY_ICONS = [
  'alarm',
  'battery',
  'bell',
  'calendar',
  'chat',
  'check-circle',
  'clock',
  'cloud',
  'coffee',
  'download',
  'envelope',
  'fire',
  'flag',
  'headphones',
  'heart',
  'hourglass',
  'info',
  'lightning',
  'list-checks',
  'map-pin',
  'microphone',
  'moon',
  'note',
  'package',
  'phone',
  'record',
  'star',
  'sun',
  'target',
  'timer',
  'trophy',
  'upload',
  'video-camera',
  'warning',
] as const;

export type ActivityIcon = (typeof ACTIVITY_ICONS)[number];

/** What one side of the mini island shows. */
export interface ActivityPart {
  icon?: ActivityIcon;
  text?: string;
  /** `#rrggbb`; the island's text color when left out. */
  color?: string;
}

export interface LiveActivity {
  /** Left end of the mini island. */
  leading?: ActivityPart;
  /** One line in the middle, cut with an ellipsis when it does not fit. */
  title?: string;
  /** Right end of the mini island. */
  trailing?: ActivityPart;
}

/** The ends are meant for an icon, a number or a word; the island also cuts them with an ellipsis. */
export const MAX_ACTIVITY_TEXT = 16;
export const MAX_ACTIVITY_TITLE = 100;

const COLOR = /^#[0-9a-f]{6}$/i;

function parseText(value: unknown, field: string, max: number): string {
  if (typeof value !== 'string') throw new Error(`${field} must be a string`);
  const clean = value.replace(/\s+/g, ' ').trim();
  if (clean.length > max) throw new Error(`${field} is longer than ${max} characters`);
  return clean;
}

function parsePart(value: unknown, side: string): ActivityPart | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'object') throw new Error(`${side} must be an object`);
  const { icon, text, color, ...rest } = value as Record<string, unknown>;
  const unknown = Object.keys(rest)[0];
  if (unknown) throw new Error(`${side} has an unknown field "${unknown}"`);
  const part: ActivityPart = {};
  if (icon !== undefined) {
    if (!ACTIVITY_ICONS.includes(icon as ActivityIcon)) throw new Error(`${side}.icon "${String(icon)}" is not a known icon`);
    part.icon = icon as ActivityIcon;
  }
  if (text !== undefined) {
    const clean = parseText(text, `${side}.text`, MAX_ACTIVITY_TEXT);
    if (clean) part.text = clean;
  }
  if (color !== undefined) {
    if (typeof color !== 'string' || !COLOR.test(color)) throw new Error(`${side}.color must look like #rrggbb`);
    part.color = color;
  }
  return part.icon || part.text ? part : undefined;
}

/** Checks what a widget sent; throws with a message the widget gets back. */
export function parseActivity(value: unknown): LiveActivity {
  if (!value || typeof value !== 'object') throw new Error('the activity must be an object');
  const { leading, title, trailing, ...rest } = value as Record<string, unknown>;
  const unknown = Object.keys(rest)[0];
  if (unknown) throw new Error(`unknown field "${unknown}"`);
  const activity: LiveActivity = {};
  const left = parsePart(leading, 'leading');
  const middle = title === undefined ? '' : parseText(title, 'title', MAX_ACTIVITY_TITLE);
  const right = parsePart(trailing, 'trailing');
  if (left) activity.leading = left;
  if (middle) activity.title = middle;
  if (right) activity.trailing = right;
  if (!left && !middle && !right) throw new Error('the activity shows nothing; call activity.clear() instead');
  return activity;
}

/** The current activity of each widget, by widget id. It stays while the widget's frame
 * reloads (for example when the tray tab is open), so the mini island does not flicker. */
class Activities {
  byWidget = $state<Record<string, LiveActivity>>({});

  set(widgetId: string, activity: LiveActivity): void {
    this.byWidget[widgetId] = activity;
  }

  clear(widgetId: string): void {
    delete this.byWidget[widgetId];
  }
}

export const activities = new Activities();
