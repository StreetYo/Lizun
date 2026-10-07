import { invoke, listen } from './platform';

/** How the island looks while collapsed (see src-tauri/src/preferences.rs). */
export type IslandVisibility = 'always' | 'autoHide' | 'hidden';

export interface Preferences {
  islandVisibility: IslandVisibility;
  /** How long the mini island stays in "hide after a delay" mode (200–10000 ms). */
  autoHideDelayMs: number;
  /** How long the cursor has to stay on the island before it opens (0–2000 ms). */
  openDelayMs: number;
  /** Accept media from the Lizun browser extension. */
  browserMedia: boolean;
  /** Port the extension connects to (1024–65535). */
  browserMediaPort: number;
}

const DEFAULTS: Preferences = {
  islandVisibility: 'always',
  autoHideDelayMs: 1000,
  openDelayMs: 100,
  browserMedia: true,
  browserMediaPort: 6869,
};

/** Preferences shared by all windows; changes made anywhere arrive through an event. */
class PreferencesStore {
  value = $state<Preferences>({ ...DEFAULTS });
  /** False until the saved preferences have been read. */
  loaded = $state(false);

  async start(): Promise<void> {
    await listen<Preferences>('preferences://changed', (next) => (this.value = next));
    this.value = { ...DEFAULTS, ...(await invoke<Preferences>('preferences_get')) };
    this.loaded = true;
  }

  async update(patch: Partial<Preferences>): Promise<void> {
    this.value = await invoke<Preferences>('preferences_set', { preferences: { ...this.value, ...patch } });
  }
}

export const preferences = new PreferencesStore();
