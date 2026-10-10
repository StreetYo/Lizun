import { invoke, listen } from '../platform';
import type { SettingValue, WidgetManifest } from './registry.svelte';

export type SettingValues = Record<string, SettingValue>;

/** A widget with settings: an installed one, or a built-in one whose settings come from the core. */
export type SettingsOwner = Pick<WidgetManifest, 'id' | 'settings'>;

/** Values of the widgets' own settings, kept by the core and shared by all windows. */
class WidgetSettingsStore {
  values = $state<Record<string, SettingValues>>({});

  #listeners = new Set<(widgetId: string) => void>();
  #started = false;

  async start(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    await listen<{ widgetId: string; values: SettingValues }>('widget-settings://changed', (change) =>
      this.#apply(change.widgetId, change.values),
    );
    await this.load();
  }

  /** Reads all values again, for example after widgets were added or removed. */
  async load(): Promise<void> {
    const all = (await invoke<Record<string, SettingValues>>('widget_settings_all')) ?? {};
    this.values = all;
    for (const widgetId of Object.keys(all)) this.#notify(widgetId);
  }

  /** A plain copy of the widget's values, with defaults for anything not loaded yet. */
  of(widget: SettingsOwner): SettingValues {
    const defaults = Object.fromEntries(widget.settings.map((setting) => [setting.key, setting.default ?? '']));
    return { ...defaults, ...this.values[widget.id] };
  }

  async set(widgetId: string, key: string, value: SettingValue): Promise<void> {
    this.#apply(widgetId, await invoke<SettingValues>('widget_settings_set', { widgetId, key, value }));
  }

  async reset(widgetId: string): Promise<void> {
    this.#apply(widgetId, await invoke<SettingValues>('widget_settings_reset', { widgetId }));
  }

  /** Called with the widget id whenever its values change. */
  onChange(listener: (widgetId: string) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  #apply(widgetId: string, values: SettingValues): void {
    this.values[widgetId] = values;
    this.#notify(widgetId);
  }

  #notify(widgetId: string): void {
    for (const listener of this.#listeners) listener(widgetId);
  }
}

export const widgetSettings = new WidgetSettingsStore();
