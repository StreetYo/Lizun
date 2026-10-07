<script lang="ts">
  import { onMount } from 'svelte';
  import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
  import ArrowClockwise from 'phosphor-svelte/lib/ArrowClockwise';
  import FolderOpen from 'phosphor-svelte/lib/FolderOpen';
  import GearSix from 'phosphor-svelte/lib/GearSix';
  import Info from 'phosphor-svelte/lib/Info';
  import PuzzlePiece from 'phosphor-svelte/lib/PuzzlePiece';
  import { invoke, isTauri } from '../lib/platform';
  import type { WidgetManifest } from '../lib/widgets/registry.svelte';

  type Section = 'general' | 'widgets' | 'about';

  const BUILT_IN = [
    { name: 'Now Playing', description: 'Cover, playback controls and progress of the current track' },
    { name: 'Mirror', description: 'Camera preview, off as soon as the island closes' },
    { name: 'Calendar', description: 'The week around today' },
    { name: 'Files Tray', description: 'Drop files on the island to keep them at hand' },
  ];

  const PERMISSIONS: Record<string, string> = {
    'media:read': 'Reads now playing',
    'media:control': 'Controls playback',
    storage: 'Stores data',
  };

  let section = $state<Section>('general');
  let launchAtLogin = $state(false);
  let widgets = $state<WidgetManifest[]>([]);
  let info = $state<{ version: string; widgetsDir: string | null }>({ version: '', widgetsDir: null });
  let error = $state<string | null>(null);

  onMount(async () => {
    info = (await invoke('app_info')) ?? info;
    widgets = (await invoke<WidgetManifest[]>('widgets_list')) ?? [];
    if (isTauri) launchAtLogin = await isEnabled().catch(() => false);
  });

  async function toggleLaunchAtLogin(): Promise<void> {
    error = null;
    try {
      if (launchAtLogin) await disable();
      else await enable();
      launchAtLogin = await isEnabled();
    } catch (err) {
      error = `Could not change the login item: ${err}`;
    }
  }

  async function reloadWidgets(): Promise<void> {
    widgets = (await invoke<WidgetManifest[]>('widgets_reload')) ?? [];
  }

  function openWidgetsFolder(): void {
    error = null;
    invoke('widgets_open_folder').catch((err) => (error = `Could not open the folder: ${err}`));
  }
</script>

<div class="window">
  <nav>
    <div class="brand"><span class="logo"></span>Lizun</div>
    <button class:active={section === 'general'} onclick={() => (section = 'general')}>
      <GearSix size={16} weight="fill" /> General
    </button>
    <button class:active={section === 'widgets'} onclick={() => (section = 'widgets')}>
      <PuzzlePiece size={16} weight="fill" /> Widgets
    </button>
    <button class:active={section === 'about'} onclick={() => (section = 'about')}>
      <Info size={16} weight="fill" /> About
    </button>
  </nav>

  <main>
    {#if section === 'general'}
      <h1>General</h1>
      <div class="card">
        <div class="row">
          <div class="text">
            <div class="label">Launch at login</div>
            <div class="hint">Start Lizun when you sign in to Windows.</div>
          </div>
          <button
            class="switch"
            class:on={launchAtLogin}
            role="switch"
            aria-checked={launchAtLogin}
            aria-label="Launch at login"
            onclick={toggleLaunchAtLogin}
          ><span></span></button>
        </div>
      </div>
    {:else if section === 'widgets'}
      <div class="head">
        <h1>Widgets</h1>
        <div class="actions">
          <button class="secondary" onclick={openWidgetsFolder}><FolderOpen size={15} weight="fill" /> Open widgets folder</button>
          <button class="secondary" onclick={reloadWidgets}><ArrowClockwise size={15} weight="bold" /> Reload</button>
        </div>
      </div>

      <h2>Built in</h2>
      <div class="card">
        {#each BUILT_IN as widget (widget.name)}
          <div class="row">
            <div class="text">
              <div class="label">{widget.name}</div>
              <div class="hint">{widget.description}</div>
            </div>
            <span class="badge">Built in</span>
          </div>
        {/each}
      </div>

      <h2>Installed</h2>
      {#if widgets.length}
        <div class="card">
          {#each widgets as widget (widget.id)}
            <div class="row">
              <div class="text">
                <div class="label">{widget.name} <span class="version">{widget.version}</span></div>
                <div class="hint">{widget.description || widget.id}</div>
                <div class="chips">
                  {#each widget.permissions as permission (permission)}
                    <span class="chip">{PERMISSIONS[permission] ?? permission}</span>
                  {/each}
                  {#each widget.network as origin (origin)}
                    <span class="chip">Network: {origin}</span>
                  {/each}
                  {#if !widget.permissions.length && !widget.network.length}
                    <span class="chip">No permissions</span>
                  {/if}
                </div>
              </div>
              <span class="badge">{widget.source === 'dev' ? 'Development' : widget.author || 'Installed'}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="card empty">No widgets installed yet. Put a widget folder into the widgets folder and press Reload.</div>
      {/if}
      {#if info.widgetsDir}<p class="path">{info.widgetsDir}</p>{/if}
    {:else}
      <h1>About</h1>
      <div class="card about">
        <span class="logo big"></span>
        <div class="text">
          <div class="label">Lizun {info.version}</div>
          <div class="hint">Open-source dynamic island with installable widgets.</div>
        </div>
      </div>
      <button class="danger" onclick={() => invoke('app_quit')}>Quit Lizun</button>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}
  </main>
</div>

<style>
  :global(html),
  :global(body) {
    background: #141316;
  }

  .window {
    display: flex;
    height: 100vh;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 190px;
    padding: 16px 10px;
    box-sizing: border-box;
    background: #1b1a1e;
    border-right: 1px solid var(--divider);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 10px 18px;
    font-size: 15px;
    font-weight: 700;
  }

  .logo {
    width: 24px;
    height: 10px;
    border-radius: 0 0 6px 6px;
    background: #000;
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.28);
  }

  .logo.big {
    flex: none;
    width: 64px;
    height: 24px;
    border-radius: 0 0 14px 14px;
  }

  nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  nav button:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.04);
  }

  nav button.active {
    color: var(--text);
    background: var(--card-raised);
  }

  main {
    flex: 1;
    overflow-y: auto;
    padding: 26px 30px;
  }

  h1 {
    margin: 0 0 18px;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  h2 {
    margin: 22px 0 8px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-tertiary);
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  .card {
    border-radius: 12px;
    background: var(--card);
    border: 1px solid var(--divider);
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 13px 16px;
  }

  .row + .row {
    border-top: 1px solid var(--divider);
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .label {
    font-size: 14px;
    font-weight: 600;
  }

  .version {
    margin-left: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-tertiary);
  }

  .hint {
    margin-top: 2px;
    font-size: 12.5px;
    color: var(--text-secondary);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }

  .chip {
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 11.5px;
    background: var(--card-raised);
    color: var(--text-secondary);
  }

  .badge {
    flex: none;
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .empty {
    padding: 18px 16px;
    font-size: 13px;
    color: var(--text-secondary);
  }

  .path {
    margin: 10px 2px 0;
    font-size: 11.5px;
    color: var(--text-tertiary);
    user-select: text;
    word-break: break-all;
  }

  .about {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 20px;
  }

  .secondary,
  .danger {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 12px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 600;
    background: var(--card-raised);
  }

  .secondary:hover,
  .danger:hover {
    filter: brightness(1.2);
  }

  .danger {
    margin-top: 16px;
    color: var(--danger);
  }

  .switch {
    flex: none;
    position: relative;
    width: 42px;
    height: 24px;
    border-radius: 12px;
    background: #3a3940;
    transition: background 160ms ease;
  }

  .switch span {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    transition: translate 200ms cubic-bezier(0.3, 1.3, 0.5, 1);
  }

  .switch.on {
    background: var(--accent);
  }

  .switch.on span {
    translate: 18px 0;
  }

  .error {
    margin-top: 14px;
    font-size: 12.5px;
    color: var(--danger);
  }
</style>
