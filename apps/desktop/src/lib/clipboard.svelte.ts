import { invoke, listen } from './platform';

/** One of the first files of a copied set (see `FileView` in src-tauri/src/clipboard.rs). */
export interface ClipboardFile {
  path: string;
  folder: boolean;
  /** Moved or deleted since it was copied. */
  missing: boolean;
}

/** An entry of the clipboard history (see `EntryView` in src-tauri/src/clipboard.rs). */
export type ClipboardEntry = {
  id: number;
  /** When it was last copied, in milliseconds since the epoch. */
  copiedAt: number;
} & (
  | { kind: 'text'; /** The start of the text. */ text: string; /** True when the text goes on. */ cut: boolean }
  | { kind: 'picture'; /** The PNG file, allowed through the asset protocol. */ path: string; width: number; height: number }
  | { kind: 'files'; /** The first few files. */ files: ClipboardFile[]; /** How many were copied. */ count: number }
);

/** Text, pictures and files the user copied, newest first. The core watches the clipboard and keeps
 * the history; this only shows it and asks for changes. */
class ClipboardHistory {
  entries = $state<ClipboardEntry[]>([]);

  #started = false;

  async start(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    await listen<ClipboardEntry[]>('clipboard://changed', (entries) => (this.entries = entries));
    try {
      this.entries = (await invoke<ClipboardEntry[]>('clipboard_history')) ?? [];
    } catch (err) {
      console.warn('could not read the clipboard history', err);
    }
  }

  /** Puts the entry back on the clipboard; it then moves to the top. */
  copy(id: number): Promise<void> {
    return invoke('clipboard_copy', { id });
  }

  remove(id: number): void {
    this.entries = this.entries.filter((entry) => entry.id !== id);
    invoke('clipboard_remove', { id }).catch((err) => console.warn('clipboard remove', err));
  }

  clear(): void {
    this.entries = [];
    invoke('clipboard_clear').catch((err) => console.warn('clipboard clear', err));
  }
}

export const clipboardHistory = new ClipboardHistory();
