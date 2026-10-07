/**
 * Island side of the widget SDK. Widgets run in sandboxed iframes with an opaque origin and
 * talk to the island with postMessage. Every request is matched to its iframe by
 * `event.source` (which a widget cannot forge) and checked against the permissions in its
 * manifest before anything is done on its behalf.
 */
import { debug } from '../debug';
import { keyboard } from '../keyboard.svelte';
import { media, type MediaCommand } from '../media.svelte';
import { activities, parseActivity } from './activities.svelte';
import type { WidgetManifest } from './registry.svelte';
import { widgetSettings } from './settings.svelte';

const PROTOCOL = 1;
const STORAGE_PREFIX = 'lizun.widget.';
const MAX_STORED_VALUE = 64 * 1024;

/** Design tokens handed to widgets so they can match the island. */
const THEME = {
  background: '#000000',
  card: '#1c1b1f',
  cardRaised: '#26252a',
  text: '#f5f5f7',
  textSecondary: '#8e8d93',
  textTertiary: '#5a5960',
  accent: '#0a84ff',
  radius: 14,
};

interface Frame {
  widget: WidgetManifest;
  iframe: HTMLIFrameElement;
  mediaSubscribed: boolean;
}

type Handler = (frame: Frame, params: Record<string, unknown>) => unknown;

const METHODS: Record<string, { permission?: string; handler: Handler }> = {
  'lizun.handshake': {
    handler: ({ widget }) => ({
      protocol: PROTOCOL,
      widgetId: widget.id,
      permissions: widget.permissions,
      theme: THEME,
      settings: widgetSettings.of(widget),
    }),
  },
  // A widget's own settings need no permission; only the user can change them.
  'settings.getAll': { handler: ({ widget }) => widgetSettings.of(widget) },
  'media.getNowPlaying': { permission: 'media:read', handler: () => mediaPayload() },
  'media.subscribe': {
    permission: 'media:read',
    handler: (frame) => {
      frame.mediaSubscribed = true;
    },
  },
  'media.control': {
    permission: 'media:control',
    handler: (_frame, params) => {
      const command = params.command as MediaCommand;
      if (!['togglePlayPause', 'next', 'previous'].includes(command)) throw new Error('unknown command');
      return media.control(command);
    },
  },
  'storage.get': {
    permission: 'storage',
    handler: ({ widget }, params) => {
      const raw = localStorage.getItem(storageKey(widget, params.key));
      return raw === null ? null : JSON.parse(raw);
    },
  },
  'storage.set': {
    permission: 'storage',
    handler: ({ widget }, params) => {
      const raw = JSON.stringify(params.value ?? null);
      if (raw.length > MAX_STORED_VALUE) throw new Error(`value exceeds ${MAX_STORED_VALUE} bytes`);
      localStorage.setItem(storageKey(widget, params.key), raw);
    },
  },
  'activity.set': {
    permission: 'activity',
    handler: ({ widget }, params) => activities.set(widget.id, parseActivity(params.activity)),
  },
  'activity.clear': { permission: 'activity', handler: ({ widget }) => activities.clear(widget.id) },
  // Sent by the SDK when a text field in the widget gains or loses focus; needs no permission.
  'keyboard.set': {
    handler: async (frame, params) => {
      const take = params.take === true;
      await keyboard.set(take);
      if (take) send(frame, { kind: 'event', name: 'keyboard.granted', payload: null });
    },
  },
  'debug.report': {
    handler: ({ widget }, params) => {
      debug.widgetReports[widget.id] = params;
      console.info(`[widget ${widget.id}]`, params);
    },
  },
};

const frames = new Map<MessageEventSource, Frame>();

function storageKey(widget: WidgetManifest, key: unknown): string {
  if (typeof key !== 'string' || !key || key.length > 128) throw new Error('invalid storage key');
  return `${STORAGE_PREFIX}${widget.id}.${key}`;
}

function mediaPayload() {
  const np = media.nowPlaying;
  if (!np) return null;
  const { artworkKey: _key, positionSampledAt: _sampled, ...rest } = np;
  return { ...rest, positionMs: media.positionAt(Date.now()), artwork: media.artwork };
}

function send(frame: Frame, message: Record<string, unknown>): void {
  // The iframe's origin is opaque, so '*' is the only usable target origin.
  frame.iframe.contentWindow?.postMessage({ __lizun: PROTOCOL, ...message }, '*');
}

async function handleMessage(event: MessageEvent): Promise<void> {
  const frame = event.source ? frames.get(event.source) : undefined;
  const message = event.data;
  if (!frame || message?.__lizun !== PROTOCOL || message.kind !== 'request') return;

  const reply = (body: Record<string, unknown>) => send(frame, { kind: 'response', id: message.id, ...body });
  const method = METHODS[message.method];
  if (!method) {
    reply({ error: { code: 'unknown_method', message: `unknown method ${String(message.method)}` } });
    return;
  }
  if (method.permission && !frame.widget.permissions.includes(method.permission)) {
    reply({
      error: {
        code: 'permission_denied',
        message: `${message.method} requires the "${method.permission}" permission in manifest.json`,
      },
    });
    return;
  }
  try {
    reply({ result: (await method.handler(frame, message.params ?? {})) ?? null });
  } catch (err) {
    reply({ error: { code: 'failed', message: err instanceof Error ? err.message : String(err) } });
  }
}

let listening = false;

/** Connects a widget iframe to the SDK bridge; returns a function that disconnects it. */
export function attachWidget(iframe: HTMLIFrameElement, widget: WidgetManifest): () => void {
  if (!listening) {
    listening = true;
    window.addEventListener('message', (event) => void handleMessage(event));
    media.onChange(() => {
      const payload = mediaPayload();
      for (const frame of frames.values()) {
        if (frame.mediaSubscribed) send(frame, { kind: 'event', name: 'media.changed', payload });
      }
    });
    // The widget's text field no longer has the keyboard, for example because the user clicked
    // another app; the SDK leaves the field so the next click takes the keyboard again.
    keyboard.onRelease(() => {
      for (const frame of frames.values()) send(frame, { kind: 'event', name: 'keyboard.released', payload: null });
    });
    widgetSettings.onChange((widgetId) => {
      for (const frame of frames.values()) {
        if (frame.widget.id !== widgetId) continue;
        send(frame, { kind: 'event', name: 'settings.changed', payload: widgetSettings.of(frame.widget) });
      }
    });
  }
  const source = iframe.contentWindow;
  if (!source) return () => {};
  frames.set(source, { widget, iframe, mediaSubscribed: false });
  return () => frames.delete(source);
}
