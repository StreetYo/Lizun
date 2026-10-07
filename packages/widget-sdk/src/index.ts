/**
 * Lizun widget SDK.
 *
 * A widget runs in a sandboxed iframe inside the island and can reach the system only
 * through these calls. The island checks each call against the `permissions` listed in the
 * widget's manifest.json and rejects the rest with a `LizunError` (code `permission_denied`).
 *
 * In a widget, import it from the island: `import { ready, media } from '/__sdk/index.js'`.
 */

export interface Theme {
  background: string;
  card: string;
  cardRaised: string;
  text: string;
  textSecondary: string;
  textTertiary: string;
  accent: string;
  radius: number;
}

/** A setting's value; its type matches the `type` declared in manifest.json. */
export type SettingValue = boolean | number | string;

/** The widget's settings by `key`, every declared key included. */
export type Settings = Record<string, SettingValue>;

export interface WidgetContext {
  protocol: number;
  widgetId: string;
  permissions: string[];
  theme: Theme;
  /** Current values of the settings declared in manifest.json. */
  settings: Settings;
}

export interface NowPlaying {
  sourceApp: string;
  title: string;
  artist: string;
  album: string;
  playing: boolean;
  positionMs: number | null;
  durationMs: number | null;
  /** Cover image as a data URL. */
  artwork: string | null;
  canToggle: boolean;
  canNext: boolean;
  canPrevious: boolean;
}

/** Icons a live activity can use. */
export type ActivityIcon =
  | 'alarm'
  | 'battery'
  | 'bell'
  | 'calendar'
  | 'chat'
  | 'check-circle'
  | 'clock'
  | 'cloud'
  | 'coffee'
  | 'download'
  | 'envelope'
  | 'fire'
  | 'flag'
  | 'headphones'
  | 'heart'
  | 'hourglass'
  | 'info'
  | 'lightning'
  | 'list-checks'
  | 'map-pin'
  | 'microphone'
  | 'moon'
  | 'note'
  | 'package'
  | 'phone'
  | 'record'
  | 'star'
  | 'sun'
  | 'target'
  | 'timer'
  | 'trophy'
  | 'upload'
  | 'video-camera'
  | 'warning';

/** What one side of the mini island shows: an icon, a short text (up to 16 characters), or both. */
export interface ActivityPart {
  icon?: ActivityIcon;
  text?: string;
  /** `#rrggbb`; the island's text color when left out. */
  color?: string;
}

/** What the widget shows in the closed island. At least one field must show something. */
export interface LiveActivity {
  /** Left end. */
  leading?: ActivityPart;
  /** One line in the middle, up to 100 characters; cut with an ellipsis when it does not fit. */
  title?: string;
  /** Right end. */
  trailing?: ActivityPart;
}

export class LizunError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'LizunError';
    this.code = code;
  }
}

const PROTOCOL = 1;

interface Pending {
  resolve(value: unknown): void;
  reject(error: Error): void;
}

let nextId = 1;
const pending = new Map<number, Pending>();
const listeners = new Map<string, Set<(payload: unknown) => void>>();

window.addEventListener('message', (event) => {
  if (event.source !== window.parent) return;
  const message = event.data;
  if (message?.__lizun !== PROTOCOL) return;

  if (message.kind === 'response') {
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new LizunError(message.error.code, message.error.message));
    else request.resolve(message.result);
  } else if (message.kind === 'event') {
    listeners.get(message.name)?.forEach((listener) => listener(message.payload));
  }
});

function call<T>(method: string, params?: Record<string, unknown>): Promise<T> {
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    window.parent.postMessage({ __lizun: PROTOCOL, kind: 'request', id, method, params }, '*');
  });
}

function on<T>(name: string, listener: (payload: T) => void): () => void {
  let set = listeners.get(name);
  if (!set) listeners.set(name, (set = new Set()));
  const wrapped = listener as (payload: unknown) => void;
  set.add(wrapped);
  return () => set.delete(wrapped);
}

const NOT_TEXT_INPUTS = new Set(['button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit']);

function isTextField(element: EventTarget | null): element is HTMLElement {
  if (element instanceof HTMLTextAreaElement) return !element.readOnly && !element.disabled;
  if (element instanceof HTMLInputElement) {
    return !element.readOnly && !element.disabled && !NOT_TEXT_INPUTS.has(element.type);
  }
  return element instanceof HTMLElement && element.isContentEditable;
}

/**
 * The island never takes the keyboard on its own, so typing would go to the app behind it. While
 * a text field in the widget is in use, the island is asked for the keyboard; when the field
 * loses focus (or Esc is pressed in it), the focus goes back to the previously active window.
 */
/** The text field this widget holds the keyboard for. */
let keyboardField: HTMLElement | null = null;

function takeKeyboard(field: HTMLElement): void {
  keyboardField = field;
  call('keyboard.set', { take: true }).catch((error) => console.warn('[lizun] no keyboard:', error));
}

function releaseKeyboard(): void {
  if (!keyboardField) return;
  const field = keyboardField;
  keyboardField = null;
  if (document.activeElement === field) field.blur();
  call('keyboard.set', { take: false }).catch(() => {});
}

function manageKeyboard(): void {
  document.addEventListener(
    'pointerdown',
    (event) => (isTextField(event.target) ? takeKeyboard(event.target) : releaseKeyboard()),
    true,
  );
  document.addEventListener('focusin', (event) => isTextField(event.target) && takeKeyboard(event.target));
  // The field keeps being the active element when the whole window loses focus; the island
  // handles that case itself.
  document.addEventListener('focusout', () => {
    setTimeout(() => !isTextField(document.activeElement) && releaseKeyboard());
  });
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && !event.defaultPrevented && isTextField(event.target)) releaseKeyboard();
  });
  // The island gave the keyboard back on its own, for example when another app was clicked.
  on('keyboard.released', () => {
    const field = keyboardField;
    keyboardField = null;
    field?.blur();
  });
  // Clicking a field focuses it before the island has the keyboard; focus it again once it has.
  on('keyboard.granted', () => {
    if (keyboardField?.isConnected && document.activeElement !== keyboardField) keyboardField.focus();
  });
}

/** Keyboard focus while a text field is in use; see `ready()`. */
export const keyboard = {
  /** Leaves the text field and gives the focus back to the app the user was working in. */
  release: releaseKeyboard,
};

/**
 * Connects to the island and applies its theme as `--lz-*` CSS variables. Call it first.
 * Text fields in the widget get the keyboard while they are in use; pressing Esc in one gives
 * the focus back to the app the user was working in.
 */
export async function ready(): Promise<WidgetContext> {
  const context = await call<WidgetContext>('lizun.handshake');
  manageKeyboard();
  const style = document.documentElement.style;
  const theme = context.theme;
  style.setProperty('--lz-bg', theme.background);
  style.setProperty('--lz-card', theme.card);
  style.setProperty('--lz-card-raised', theme.cardRaised);
  style.setProperty('--lz-text', theme.text);
  style.setProperty('--lz-text-secondary', theme.textSecondary);
  style.setProperty('--lz-text-tertiary', theme.textTertiary);
  style.setProperty('--lz-accent', theme.accent);
  style.setProperty('--lz-radius', `${theme.radius}px`);
  return context;
}

/** Now playing. Reading needs `media:read`; controlling playback needs `media:control`. */
export const media = {
  getNowPlaying: () => call<NowPlaying | null>('media.getNowPlaying'),
  subscribe(listener: (nowPlaying: NowPlaying | null) => void): () => void {
    const off = on('media.changed', listener);
    call('media.subscribe').catch((error) => console.warn('[lizun] media.subscribe failed:', error));
    return off;
  },
  togglePlayPause: () => call<void>('media.control', { command: 'togglePlayPause' }),
  next: () => call<void>('media.control', { command: 'next' }),
  previous: () => call<void>('media.control', { command: 'previous' }),
};

/**
 * The widget's own settings, declared in manifest.json and changed by the user in Lizun's
 * settings window. Widgets can only read them; no permission is needed.
 */
export const settings = {
  getAll: () => call<Settings>('settings.getAll'),
  /** Called with all values whenever the user changes any of them. */
  subscribe: (listener: (settings: Settings) => void): (() => void) => on('settings.changed', listener),
};

/**
 * The widget's live activity in the closed island. Needs `activity`. The island shows one
 * activity at a time: the current track and files in the tray come first, then widgets in the
 * order the user put them in. `set` replaces the previous activity; call `clear` when there is
 * nothing to show. The activity stays while the widget reloads, so set or clear it on start.
 */
export const activity = {
  set: (activity: LiveActivity) => call<void>('activity.set', { activity }),
  clear: () => call<void>('activity.clear'),
};

/** Small JSON values that survive restarts, private to this widget. Needs `storage`. */
export const storage = {
  get: <T = unknown>(key: string) => call<T | null>('storage.get', { key }),
  set: (key: string, value: unknown) => call<void>('storage.set', { key, value }),
};

/** Development helper: the report shows up in the island's console. */
export const debug = {
  report: (data: Record<string, unknown>) => call<void>('debug.report', data),
};
