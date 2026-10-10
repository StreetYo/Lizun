import { forLanguage } from '../i18n.svelte';
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
  | (SettingBase & {
      type: 'text';
      default: string;
      maxLength: number;
      placeholder: string;
      /** Built-in widgets only: a button fills the field with the path of a file picked in a dialog. */
      file?: { name: string; extensions: string[] };
    });

/** A setting's texts in another language (see src-tauri/src/widget_settings.rs). */
export interface SettingTexts {
  label?: string;
  description?: string;
  placeholder?: string;
  unit?: string;
  /** Option labels by option value. */
  options?: Record<string, string>;
}

/** A manifest's texts in another language (see docs/widgets.md#languages). */
export interface ManifestTexts {
  name?: string;
  description?: string;
  settings?: Record<string, SettingTexts>;
}

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
  /** Programs the widget may start (see src-tauri/src/widget_apps.rs). */
  apps?: { id: string; name: string; paths: string[] }[];
  settings: SettingSpec[];
  /** Translations by language code. */
  locales?: Record<string, ManifestTexts>;
  source: 'bundled' | 'dev' | 'user';
}

/** The manifest with its texts in the current language, where it has them. */
export function localized<T extends Pick<WidgetManifest, 'name' | 'description' | 'settings' | 'locales'>>(manifest: T): T {
  const texts = forLanguage(manifest.locales);
  if (!texts) return manifest;
  return {
    ...manifest,
    name: texts.name ?? manifest.name,
    description: texts.description ?? manifest.description,
    settings: manifest.settings.map((setting) => localizedSetting(setting, texts.settings?.[setting.key])),
  };
}

function localizedSetting(setting: SettingSpec, texts: SettingTexts | undefined): SettingSpec {
  if (!texts) return setting;
  const common = { label: texts.label ?? setting.label, description: texts.description ?? setting.description };
  switch (setting.type) {
    case 'number':
      return { ...setting, ...common, unit: texts.unit ?? setting.unit };
    case 'text':
      return { ...setting, ...common, placeholder: texts.placeholder ?? setting.placeholder };
    case 'select':
      return {
        ...setting,
        ...common,
        options: setting.options.map((option) => ({ ...option, label: texts.options?.[option.value] ?? option.label })),
      };
    default:
      return { ...setting, ...common };
  }
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
