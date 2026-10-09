import { t } from '../i18n.svelte';
import { preferences, WIDGET_ROWS, type WidgetSlot } from '../preferences.svelte';
import { localized, type WidgetManifest } from './registry.svelte';

/** Widgets that are part of the app, with the name of their texts in the locale files. A colon
 * cannot appear in a widget id, so an installed widget never takes one of these. */
export const BUILT_IN_WIDGETS = [
  { id: 'builtin:media', texts: 'media' },
  { id: 'builtin:mirror', texts: 'mirror' },
  { id: 'builtin:calendar', texts: 'calendar' },
] as const;

export type BuiltInWidgetId = (typeof BUILT_IN_WIDGETS)[number]['id'];

/** Name and description of a part of the app, in the current language. */
export function builtInTexts(texts: (typeof BUILT_IN_WIDGETS)[number]['texts'] | 'tray'): { name: string; description: string } {
  return { name: t(`builtIn.${texts}.name`), description: t(`builtIn.${texts}.description`) };
}

export interface LayoutEntry {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  /** Row of the island the widget is in, counted from 0. */
  row: number;
  /** Null for a built-in widget. */
  manifest: WidgetManifest | null;
}

/** All widgets in island order, row by row, named in the current language: the saved order
 * first, then widgets the user has not placed yet (built-in ones before installed ones), which
 * are shown in the first row. */
export function widgetLayout(installed: WidgetManifest[], slots: WidgetSlot[] = preferences.value.widgets): LayoutEntry[] {
  const available: LayoutEntry[] = [
    ...BUILT_IN_WIDGETS.map((widget) => ({ id: widget.id, ...builtInTexts(widget.texts), enabled: true, row: 0, manifest: null })),
    ...installed.map(localized).map((manifest) => ({
      id: manifest.id,
      name: manifest.name,
      description: manifest.description || manifest.id,
      enabled: true,
      row: 0,
      manifest,
    })),
  ];
  const byId = new Map(available.map((entry) => [entry.id, entry]));
  const placed = slots.flatMap((slot) => {
    const entry = byId.get(slot.id);
    if (!entry) return [];
    byId.delete(slot.id);
    const row = Math.min(WIDGET_ROWS - 1, Math.max(0, Math.floor(slot.row ?? 0)));
    return [{ ...entry, enabled: slot.enabled, row }];
  });
  // The sort is stable, so each row keeps the saved order.
  return [...placed, ...byId.values()].sort((a, b) => a.row - b.row);
}

/** Saves the order, rows and visibility of the widgets in `layout`. Saved widgets that are not
 * installed right now keep their place in the list, so one that comes back is where it was. */
export function saveLayout(layout: Pick<LayoutEntry, 'id' | 'enabled' | 'row'>[]): Promise<void> {
  const next = layout.map(({ id, enabled, row }) => ({ id, enabled, row }));
  const ids = new Set(next.map((slot) => slot.id));
  let taken = 0;
  const widgets = preferences.value.widgets.map((slot) => (ids.has(slot.id) ? next[taken++] : slot));
  widgets.push(...next.slice(taken));
  return preferences.update({ widgets });
}

/** Saves `rows`, which hold every widget of `widgetLayout()` row by row. */
export function arrangeWidgets(rows: LayoutEntry[][]): Promise<void> {
  return saveLayout(rows.flatMap((entries, row) => entries.map((entry) => ({ ...entry, row }))));
}

/** Moves a widget into `row`, in front of the widget `before`, or after the last widget of that
 * row when `before` is null. */
export function placeWidget(layout: LayoutEntry[], id: string, row: number, before: string | null): Promise<void> {
  const moved = layout.find((entry) => entry.id === id);
  if (!moved) return Promise.resolve();
  const next = layout.filter((entry) => entry !== moved);
  let at = before === null ? -1 : next.findIndex((entry) => entry.id === before);
  if (at < 0) {
    at = next.length;
    while (at > 0 && next[at - 1].row > row) at--;
  }
  next.splice(at, 0, { ...moved, row });
  return saveLayout(next);
}

export function setWidgetEnabled(layout: LayoutEntry[], id: string, enabled: boolean): Promise<void> {
  return saveLayout(layout.map((entry) => (entry.id === id ? { ...entry, enabled } : entry)));
}
