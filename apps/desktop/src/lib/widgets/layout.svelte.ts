import { preferences, type WidgetSlot } from '../preferences.svelte';
import type { WidgetManifest } from './registry.svelte';

/** Widgets that are part of the app. A colon cannot appear in a widget id, so an installed
 * widget never takes one of these. */
export const BUILT_IN_WIDGETS = [
  { id: 'builtin:media', name: 'Now Playing', description: 'Cover, playback controls and progress of the current track' },
  { id: 'builtin:mirror', name: 'Mirror', description: 'Camera preview, off as soon as the island closes' },
  { id: 'builtin:calendar', name: 'Calendar', description: 'The week around today, with events from your calendar link or file' },
] as const;

export type BuiltInWidgetId = (typeof BUILT_IN_WIDGETS)[number]['id'];

export interface LayoutEntry {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  /** Null for a built-in widget. */
  manifest: WidgetManifest | null;
}

/** All widgets in island order: the saved order first, then widgets the user has not placed
 * yet (built-in ones before installed ones), which are shown. */
export function widgetLayout(installed: WidgetManifest[], slots: WidgetSlot[] = preferences.value.widgets): LayoutEntry[] {
  const available: LayoutEntry[] = [
    ...BUILT_IN_WIDGETS.map((widget) => ({ ...widget, enabled: true, manifest: null })),
    ...installed.map((manifest) => ({
      id: manifest.id,
      name: manifest.name,
      description: manifest.description || manifest.id,
      enabled: true,
      manifest,
    })),
  ];
  const byId = new Map(available.map((entry) => [entry.id, entry]));
  const placed = slots.flatMap((slot) => {
    const entry = byId.get(slot.id);
    if (!entry) return [];
    byId.delete(slot.id);
    return [{ ...entry, enabled: slot.enabled }];
  });
  return [...placed, ...byId.values()];
}

/** Saves the order and visibility of the widgets in `layout`. Saved widgets that are not
 * installed right now keep their place in the list, so one that comes back is where it was. */
export function saveLayout(layout: Pick<LayoutEntry, 'id' | 'enabled'>[]): Promise<void> {
  const next = layout.map(({ id, enabled }) => ({ id, enabled }));
  const ids = new Set(next.map((slot) => slot.id));
  let taken = 0;
  const widgets = preferences.value.widgets.map((slot) => (ids.has(slot.id) ? next[taken++] : slot));
  widgets.push(...next.slice(taken));
  return preferences.update({ widgets });
}

export function moveWidget(layout: LayoutEntry[], from: number, to: number): Promise<void> {
  const next = [...layout];
  next.splice(to, 0, ...next.splice(from, 1));
  return saveLayout(next);
}

export function setWidgetEnabled(layout: LayoutEntry[], id: string, enabled: boolean): Promise<void> {
  return saveLayout(layout.map((entry) => (entry.id === id ? { ...entry, enabled } : entry)));
}
