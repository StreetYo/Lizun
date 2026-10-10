import { open } from '@tauri-apps/plugin-dialog';
import { t } from '../i18n.svelte';
import { invoke, isTauri } from '../platform';
import { localized, type WidgetManifest } from './registry.svelte';

/** Where to install a widget from (see src-tauri/src/widget_install.rs). */
export type InstallSource = { kind: 'github'; url: string } | { kind: 'path'; path: string };

/** A widget that was downloaded and validated and waits for the user to agree. */
export interface PendingInstall {
  token: number;
  manifest: WidgetManifest;
  origin: string;
  /** Version of the installed widget this one replaces. */
  replaces: string | null;
  /** Where a widget with this id was installed from before: `github.com/<owner>/<repo>` or
   * `local`, or null when Lizun does not know. */
  previousSource: string | null;
  /** The widget may not be by the author of the one Lizun had with this id; installing it resets
   * that widget's settings. */
  sourceChanged: boolean;
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
        ? { title: t('settings.widgets.chooseArchive'), filters: [{ name: t('settings.widgets.archiveType'), extensions: ['zip'] }] }
        : { title: t('settings.widgets.chooseFolder'), directory: true },
    );
    if (typeof path === 'string') await this.prepare({ kind: 'path', path });
  }

  async confirm(): Promise<void> {
    const pending = this.pending;
    if (!pending || this.busy) return;
    this.busy = 'installing';
    this.error = null;
    try {
      // The core deletes the earlier widget's settings and data; values still waiting in
      // localStorage to move to the core (see moveLegacyValues in host.ts) go too, by the
      // same rule, so the newcomer cannot pick them up.
      if (pending.sourceChanged) forgetLegacyStorage(pending.manifest.id);
      const widgets = await invoke<WidgetManifest[]>('widgets_install_confirm', { token: pending.token });
      const { name, version } = localized(pending.manifest);
      this.done = pending.replaces
        ? t('settings.widgets.wasUpdated', { name, version })
        : t('settings.widgets.wasInstalled', { name });
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

function forgetLegacyStorage(id: string): void {
  try {
    const prefix = `lizun.widget.${id}.`;
    for (const name of Object.keys(localStorage)) {
      if (name.startsWith(prefix) && !name.slice(prefix.length).includes('.')) localStorage.removeItem(name);
    }
  } catch {
    // Storage can be unavailable; then there is nothing to move either.
  }
}
