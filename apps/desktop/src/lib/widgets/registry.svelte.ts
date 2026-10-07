import { invoke } from '../platform';

export type SettingValue = boolean | number | string;

interface SettingBase {
  key: string;
  label: string;
  description: string;
}

/** One option a widget declares in its manifest (see src-tauri/src/widget_settings.rs). */
export type SettingSpec =
  | (SettingBase & { type: 'toggle'; default: boolean })
  | (SettingBase & { type: 'number'; default: number; min: number; max: number; step: number; unit: string })
  | (SettingBase & { type: 'select'; default: string; options: { value: string; label: string }[] })
  | (SettingBase & { type: 'text'; default: string; maxLength: number; placeholder: string });

export interface WidgetManifest {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  entry: string;
  width: number;
  permissions: string[];
  network: string[];
  settings: SettingSpec[];
  source: 'bundled' | 'dev' | 'user';
}

class WidgetRegistry {
  widgets = $state<WidgetManifest[]>([]);

  async load(): Promise<void> {
    try {
      this.widgets = (await invoke<WidgetManifest[]>('widgets_list')) ?? [];
    } catch (err) {
      console.warn('could not list widgets', err);
    }
  }
}

export const widgetRegistry = new WidgetRegistry();
