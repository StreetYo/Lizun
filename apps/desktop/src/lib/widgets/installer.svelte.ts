import { open } from '@tauri-apps/plugin-dialog';
import { invoke, isTauri } from '../platform';
import type { WidgetManifest } from './registry.svelte';

/** Where to install a widget from (see src-tauri/src/widget_install.rs). */
export type InstallSource = { kind: 'github'; url: string } | { kind: 'path'; path: string };

/** A widget that was downloaded and validated and waits for the user to agree. */
export interface PendingInstall {
  token: number;
  manifest: WidgetManifest;
  origin: string;
  /** Version of the installed widget this one replaces. */
  replaces: string | null;
}

/** Installs a widget in two steps: download and check it, then install it once the user has
 * seen what it asks for. */
export class WidgetInstaller {
  busy = $state<'checking' | 'installing' | null>(null);
  pending = $state<PendingInstall | null>(null);
  error = $state<string | null>(null);
  done = $state<string | null>(null);

  constructor(private readonly installed: (widgets: WidgetManifest[]) => void) {}

  async prepare(source: InstallSource): Promise<void> {
    if (this.busy) return;
    this.cancel();
    this.error = this.done = null;
    this.busy = 'checking';
    try {
      this.pending = await invoke<PendingInstall>('widgets_install_prepare', { source });
    } catch (err) {
      this.error = String(err);
    } finally {
      this.busy = null;
    }
  }

  async choose(kind: 'zip' | 'folder'): Promise<void> {
    if (!isTauri) return this.prepare({ kind: 'path', path: kind === 'zip' ? 'C:\\Downloads\\clock.zip' : 'C:\\Widgets\\clock' });
    const path = await open(
      kind === 'zip'
        ? { title: 'Choose a widget archive', filters: [{ name: 'Widget archive', extensions: ['zip'] }] }
        : { title: 'Choose a widget folder', directory: true },
    );
    if (typeof path === 'string') await this.prepare({ kind: 'path', path });
  }

  async confirm(): Promise<void> {
    const pending = this.pending;
    if (!pending || this.busy) return;
    this.busy = 'installing';
    this.error = null;
    try {
      const widgets = await invoke<WidgetManifest[]>('widgets_install_confirm', { token: pending.token });
      const { name, version } = pending.manifest;
      this.done = pending.replaces ? `${name} was updated to ${version}.` : `${name} was installed and is in the island now.`;
      this.installed(widgets);
    } catch (err) {
      this.error = String(err);
    } finally {
      // The backend lets go of the staged widget either way.
      this.pending = null;
      this.busy = null;
    }
  }

  cancel(): void {
    if (!this.pending) return;
    invoke('widgets_install_cancel', { token: this.pending.token }).catch(() => {});
    this.pending = null;
  }
}
