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

/** Connects to the island and applies its theme as `--lz-*` CSS variables. Call it first. */
export async function ready(): Promise<WidgetContext> {
  const context = await call<WidgetContext>('lizun.handshake');
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

/** Small JSON values that survive restarts, private to this widget. Needs `storage`. */
export const storage = {
  get: <T = unknown>(key: string) => call<T | null>('storage.get', { key }),
  set: (key: string, value: unknown) => call<void>('storage.set', { key, value }),
};

/** Development helper: the report shows up in the island's console. */
export const debug = {
  report: (data: Record<string, unknown>) => call<void>('debug.report', data),
};
