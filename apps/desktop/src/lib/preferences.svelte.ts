import { invoke, listen } from './platform';

/** How the island looks while collapsed (see src-tauri/src/preferences.rs). */
export type IslandVisibility = 'always' | 'autoHide' | 'hidden';

/** A widget's place in the island (see src-tauri/src/preferences.rs). */
export interface WidgetSlot {
  id: string;
  enabled: boolean;
  /** Row of the island, counted from 0 (below `WIDGET_ROWS`). */
  row: number;
}

/** The island shows its widgets in up to this many rows (see src-tauri/src/preferences.rs). */
export const WIDGET_ROWS = 2;

export interface Preferences {
  /** Interface language: a code such as `ru`, or `system` to follow Windows (see i18n.svelte.ts). */
  language: string;
  islandVisibility: IslandVisibility;
  /** How long the mini island stays in "hide after a delay" mode (200–10000 ms). */
  autoHideDelayMs: number;
  /** In "hide after a delay" mode, show the mini island when a track starts, pauses or changes. */
  showOnMediaChange: boolean;
  /** How long the cursor has to stay on the island before it opens (0–2000 ms). */
  openDelayMs: number;
  /** Accept media from the Lizun browser extension. */
  browserMedia: boolean;
  /** Port the extension connects to (1024–65535). */
  browserMediaPort: number;
  /** Order of the widgets in the island and which of them are shown (see lib/widgets/layout.svelte.ts). */
  widgets: WidgetSlot[];
  /** Keep a history of copied text and pictures for the island's Clipboard tab. */
  clipboardHistory: boolean;
  /** Which kinds of clips the history keeps. A copy counts as its first kind: files, text, picture. */
  clipboardText: boolean;
  clipboardPictures: boolean;
  clipboardFiles: boolean;
  /** How long history entries stay, in hours (0–8760); 0 keeps them until the limit pushes them out. */
  clipboardLifetimeHours: number;
  /** Whether the history keeps at most `clipboardLimit` entries. */
  clipboardLimitEnabled: boolean;
  /** Most entries the history keeps when it is limited (1–1000). */
  clipboardLimit: number;
}

const DEFAULTS: Preferences = {
  language: 'system',
  islandVisibility: 'always',
  autoHideDelayMs: 1000,
  showOnMediaChange: true,
  openDelayMs: 100,
  browserMedia: true,
  browserMediaPort: 6869,
  widgets: [],
  clipboardHistory: true,
  clipboardText: true,
  clipboardPictures: true,
  clipboardFiles: true,
  clipboardLifetimeHours: 24,
  clipboardLimitEnabled: true,
  clipboardLimit: 50,
};

/** Preferences shared by all windows; changes made anywhere arrive through an event. */
class PreferencesStore {
  value = $state<Preferences>({ ...DEFAULTS });
  /** False until the saved preferences have been read. */
  loaded = $state(false);

  #started: Promise<void> | null = null;

  /** Reads the saved preferences once; later calls wait for the same read. */
  start(): Promise<void> {
    this.#started ??= (async () => {
      await listen<Preferences>('preferences://changed', (next) => (this.value = next));
      this.value = { ...DEFAULTS, ...(await invoke<Preferences>('preferences_get')) };
      this.loaded = true;
    })();
    return this.#started;
  }

  /** The change shows at once (a dropped widget must not jump back while it is saved) and is
   * undone if saving fails. */
  async update(patch: Partial<Preferences>): Promise<void> {
    const previous = this.value;
    this.value = { ...previous, ...patch };
    try {
      this.value = await invoke<Preferences>('preferences_set', { preferences: this.value });
    } catch (err) {
      this.value = previous;
      throw err;
    }
  }
}

export const preferences = new PreferencesStore();
