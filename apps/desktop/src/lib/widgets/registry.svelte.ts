import { invoke } from '../platform';

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
  source: 'dev' | 'user';
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
