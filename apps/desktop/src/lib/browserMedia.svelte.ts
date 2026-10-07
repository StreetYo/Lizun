import { invoke, listen } from './platform';

/** State of the browser extension server (see src-tauri/src/browser.rs). */
export interface BrowserMediaStatus {
  enabled: boolean;
  port: number;
  listening: boolean;
  /** Why the server is not listening, such as the port being used by another app. */
  error: string | null;
  /** Browsers connected right now. */
  browsers: string[];
}

class BrowserMediaStore {
  status = $state<BrowserMediaStatus | null>(null);
  #started = false;

  async start(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    await listen<BrowserMediaStatus>('browser-media://status', (status) => (this.status = status));
    this.status = (await invoke<BrowserMediaStatus>('browser_media_status')) ?? null;
  }
}

export const browserMedia = new BrowserMediaStore();
