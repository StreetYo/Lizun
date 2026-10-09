<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
  import ArrowClockwise from 'phosphor-svelte/lib/ArrowClockwise';
  import CircleNotch from 'phosphor-svelte/lib/CircleNotch';
  import DotsSixVertical from 'phosphor-svelte/lib/DotsSixVertical';
  import FileZip from 'phosphor-svelte/lib/FileZip';
  import Folder from 'phosphor-svelte/lib/Folder';
  import FolderOpen from 'phosphor-svelte/lib/FolderOpen';
  import GearSix from 'phosphor-svelte/lib/GearSix';
  import GithubLogo from 'phosphor-svelte/lib/GithubLogo';
  import Info from 'phosphor-svelte/lib/Info';
  import MusicNotes from 'phosphor-svelte/lib/MusicNotes';
  import PuzzlePiece from 'phosphor-svelte/lib/PuzzlePiece';
  import Trash from 'phosphor-svelte/lib/Trash';
  import { browserMedia } from '../lib/browserMedia.svelte';
  import { invoke, isTauri, onFileDrag } from '../lib/platform';
  import { preferences, type IslandVisibility } from '../lib/preferences.svelte';
  import { Reorder } from '../lib/reorder.svelte';
  import { WidgetInstaller } from '../lib/widgets/installer.svelte';
  import { BUILT_IN_WIDGETS, moveWidget, setWidgetEnabled, widgetLayout } from '../lib/widgets/layout.svelte';
  import type { SettingSpec, SettingValue, WidgetManifest } from '../lib/widgets/registry.svelte';
  import { widgetSettings } from '../lib/widgets/settings.svelte';

  type Section = 'general' | 'media' | 'widgets' | 'about';

  const VISIBILITY: { value: IslandVisibility; label: string; hint: string }[] = [
    {
      value: 'always',
      label: 'Always visible',
      hint: 'The mini island stays at the top of the screen while it has something to show, such as music, files in the tray or a widget activity.',
    },
    {
      value: 'autoHide',
      label: 'Hide after a delay',
      hint: 'Shows after the island closes or when something changes, such as a new track, then hides.',
    },
    { value: 'hidden', label: 'Hidden', hint: 'Nothing is shown while the island is closed.' },
  ];

  const BUILT_IN = [
    ...BUILT_IN_WIDGETS,
    { id: 'tray', name: 'Files Tray', description: 'Drop files on the island to keep them at hand' },
  ];

  const PERMISSIONS: Record<string, string> = {
    'media:read': 'Reads now playing',
    'media:control': 'Controls playback',
    storage: 'Stores data',
    activity: 'Shows in the closed island',
  };

  let section = $state<Section>('general');
  let launchAtLogin = $state(false);
  let widgets = $state<WidgetManifest[]>([]);
  // Widgets shipped with Lizun are listed with the other built-in ones.
  const bundledWidgets = $derived(widgets.filter((widget) => widget.source === 'bundled'));
  const installedWidgets = $derived(widgets.filter((widget) => widget.source !== 'bundled'));
  let info = $state<{ version: string; widgetsDir: string | null }>({ version: '', widgetsDir: null });
  let error = $state<string | null>(null);

  onMount(() => {
    void preferences.start();
    void browserMedia.start();
    void widgetSettings.start();
    void (async () => {
      info = (await invoke('app_info')) ?? info;
      widgets = (await invoke<WidgetManifest[]>('widgets_list')) ?? [];
      if (isTauri) launchAtLogin = await isEnabled().catch(() => false);
    })();
    // A zip archive or a folder dropped anywhere on the window is installed as a widget.
    const stop = onFileDrag({
      enter: () => (dropping = true),
      over: () => {},
      leave: () => (dropping = false),
      drop: (paths) => {
        dropping = false;
        section = 'widgets';
        if (paths.length === 1) void installer.prepare({ kind: 'path', path: paths[0] });
        else installer.error = 'Drop one widget at a time.';
      },
    });
    return () => {
      installer.cancel();
      void stop.then((unlisten) => unlisten());
    };
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

  async function setVisibility(value: IslandVisibility): Promise<void> {
    error = null;
    try {
      await preferences.update({ islandVisibility: value });
    } catch (err) {
      error = `Could not save the setting: ${err}`;
    }
  }

  /** Allowed ranges in seconds; they match src-tauri/src/preferences.rs. */
  const AUTO_HIDE_DELAY_S = { min: 0.2, max: 10 };
  const OPEN_DELAY_S = { min: 0, max: 2 };

  /** Saves a delay typed in seconds, clamped to its range; anything else restores the saved value. */
  async function setDelay(
    event: Event,
    key: 'autoHideDelayMs' | 'openDelayMs',
    range: { min: number; max: number },
  ): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const seconds = Number.parseFloat(input.value.replace(',', '.'));
    if (!Number.isFinite(seconds)) {
      input.value = String(preferences.value[key] / 1000);
      return;
    }
    const clamped = Math.min(range.max, Math.max(range.min, seconds));
    input.value = String(clamped);
    error = null;
    try {
      await preferences.update({ [key]: Math.round(clamped * 1000) });
    } catch (err) {
      error = `Could not save the setting: ${err}`;
    }
  }

  /** Allowed port range; it matches src-tauri/src/preferences.rs. */
  const PORTS = { min: 1024, max: 65535 };

  async function toggle(key: 'browserMedia' | 'showOnMediaChange'): Promise<void> {
    error = null;
    try {
      await preferences.update({ [key]: !preferences.value[key] });
    } catch (err) {
      error = `Could not save the setting: ${err}`;
    }
  }

  async function setPort(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const port = Number.parseInt(input.value, 10);
    if (!Number.isInteger(port) || port < PORTS.min || port > PORTS.max) {
      input.value = String(preferences.value.browserMediaPort);
      error = `The port must be a number from ${PORTS.min} to ${PORTS.max}.`;
      return;
    }
    error = null;
    try {
      await preferences.update({ browserMediaPort: port });
    } catch (err) {
      error = `Could not save the setting: ${err}`;
    }
  }

  const browserMediaState = $derived.by((): { text: string; tone: 'ok' | 'wait' | 'bad' } => {
    const status = browserMedia.status;
    if (!status) return { text: 'Starting…', tone: 'wait' };
    if (status.error) return { text: `${status.error} Lizun keeps retrying; choose another port if it stays busy.`, tone: 'bad' };
    if (status.browsers.length) return { text: `Connected: ${[...new Set(status.browsers)].join(', ')}`, tone: 'ok' };
    if (status.listening) return { text: `Waiting for the extension on port ${status.port}.`, tone: 'wait' };
    return { text: 'Starting…', tone: 'wait' };
  });

  async function reloadWidgets(): Promise<void> {
    widgets = (await invoke<WidgetManifest[]>('widgets_reload')) ?? [];
    await widgetSettings.load();
  }

  async function setWidgetSetting(widget: WidgetManifest, key: string, value: SettingValue): Promise<void> {
    error = null;
    try {
      await widgetSettings.set(widget.id, key, value);
    } catch (err) {
      error = `Could not save the setting: ${err}`;
    }
  }

  /** Numbers are clamped to the declared range; anything that is not a number restores the saved value. */
  async function setWidgetNumber(
    event: Event,
    widget: WidgetManifest,
    setting: Extract<SettingSpec, { type: 'number' }>,
  ): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const number = Number.parseFloat(input.value.replace(',', '.'));
    if (!Number.isFinite(number)) {
      input.value = String(widgetSettings.of(widget)[setting.key]);
      return;
    }
    const clamped = Math.min(setting.max, Math.max(setting.min, number));
    input.value = String(clamped);
    await setWidgetSetting(widget, setting.key, clamped);
  }

  async function resetWidgetSettings(widget: WidgetManifest): Promise<void> {
    error = null;
    try {
      await widgetSettings.reset(widget.id);
    } catch (err) {
      error = `Could not reset the settings: ${err}`;
    }
  }

  const layout = $derived(widgetLayout(widgets, preferences.value.widgets));
  let layoutList = $state<HTMLElement>();
  const reorder = new Reorder('y', 0, (from, to) => void moveLayout(from, to));

  function layoutRows(): HTMLElement[] {
    return [...(layoutList?.querySelectorAll<HTMLElement>('.layout-row') ?? [])];
  }

  async function moveLayout(from: number, to: number, refocus = false): Promise<void> {
    error = null;
    try {
      await moveWidget(layout, from, to);
    } catch (err) {
      error = `Could not save the order: ${err}`;
    }
    // The row was moved in the page, which takes the focus away from its handle.
    if (refocus) {
      await tick();
      layoutRows()[to]?.querySelector<HTMLElement>('.grip')?.focus();
    }
  }

  function layoutKey(event: KeyboardEvent, index: number): void {
    const to = event.key === 'ArrowUp' ? index - 1 : event.key === 'ArrowDown' ? index + 1 : null;
    if (to === null) return;
    event.preventDefault();
    if (to >= 0 && to < layout.length) void moveLayout(index, to, true);
  }

  async function toggleWidget(id: string, enabled: boolean): Promise<void> {
    error = null;
    try {
      await setWidgetEnabled(layout, id, enabled);
    } catch (err) {
      error = `Could not save the setting: ${err}`;
    }
  }

  const installer = new WidgetInstaller((list) => {
    widgets = list;
    void widgetSettings.load();
  });
  let link = $state('');
  let dropping = $state(false);
  /** Id of the widget whose removal waits for a second click. */
  let removing = $state<string | null>(null);

  function installFromLink(event: SubmitEvent): void {
    event.preventDefault();
    if (link.trim()) void installer.prepare({ kind: 'github', url: link.trim() });
  }

  async function confirmInstall(): Promise<void> {
    await installer.confirm();
    if (installer.done) link = '';
  }

  async function uninstall(widget: WidgetManifest): Promise<void> {
    error = null;
    removing = null;
    try {
      widgets = await invoke<WidgetManifest[]>('widgets_uninstall', { id: widget.id });
    } catch (err) {
      error = `Could not remove ${widget.name}: ${err}`;
    }
  }

  function openWidgetsFolder(): void {
    error = null;
    invoke('widgets_open_folder').catch((err) => (error = `Could not open the folder: ${err}`));
  }
</script>

{#snippet widgetSettingRows(widget: WidgetManifest)}
  {#if widget.settings.length}
    {@const values = widgetSettings.of(widget)}
    {#each widget.settings as setting (setting.key)}
      {@render settingRow(widget, setting, values[setting.key])}
    {/each}
    <div class="row setting reset">
      <button class="link" onclick={() => resetWidgetSettings(widget)}>Reset to defaults</button>
    </div>
  {/if}
{/snippet}

{#snippet permissionChips(widget: WidgetManifest)}
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
{/snippet}

{#snippet settingRow(widget: WidgetManifest, setting: SettingSpec, value: SettingValue)}
  {@const id = `setting-${widget.id}-${setting.key}`}
  <div class="row setting">
    <label class="text" for={id}>
      <span class="label">{setting.label}</span>
      {#if setting.description}<span class="hint">{setting.description}</span>{/if}
    </label>
    {#if setting.type === 'toggle'}
      <button
        {id}
        class="switch"
        class:on={value === true}
        role="switch"
        aria-checked={value === true}
        aria-label={setting.label}
        onclick={() => setWidgetSetting(widget, setting.key, value !== true)}
      ><span></span></button>
    {:else if setting.type === 'number'}
      <span class="field">
        <input
          {id}
          type="number"
          min={setting.min}
          max={setting.max}
          step={setting.step}
          {value}
          onchange={(event) => setWidgetNumber(event, widget, setting)}
        />
        {#if setting.unit}<span class="unit">{setting.unit}</span>{/if}
      </span>
    {:else if setting.type === 'select'}
      <select {id} class="select" {value} onchange={(event) => setWidgetSetting(widget, setting.key, event.currentTarget.value)}>
        {#each setting.options as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    {:else}
      <input
        {id}
        class="text-input"
        type="text"
        maxlength={setting.maxLength}
        placeholder={setting.placeholder}
        {value}
        onchange={(event) => setWidgetSetting(widget, setting.key, event.currentTarget.value)}
      />
    {/if}
  </div>
{/snippet}

<div class="window">
  <nav>
    <div class="brand"><span class="logo"></span>Lizun</div>
    <button class:active={section === 'general'} onclick={() => (section = 'general')}>
      <GearSix size={16} weight="fill" /> General
    </button>
    <button class:active={section === 'media'} onclick={() => (section = 'media')}>
      <MusicNotes size={16} weight="fill" /> Media
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

      <h2>Mini island</h2>
      <div class="card" role="radiogroup" aria-label="Mini island">
        {#each VISIBILITY as option (option.value)}
          {@const checked = preferences.value.islandVisibility === option.value}
          <button class="row option" role="radio" aria-checked={checked} onclick={() => setVisibility(option.value)}>
            <span class="radio" class:checked></span>
            <span class="text">
              <span class="label">{option.label}</span>
              <span class="hint">{option.hint}</span>
            </span>
          </button>
          {#if option.value === 'autoHide' && checked}
            <div class="row delay">
              <label class="text" for="auto-hide-delay">
                <span class="label">Delay</span>
                <span class="hint">
                  How long the mini island stays, from {AUTO_HIDE_DELAY_S.min} to {AUTO_HIDE_DELAY_S.max} seconds.
                </span>
              </label>
              <span class="field">
                <input
                  id="auto-hide-delay"
                  type="number"
                  min={AUTO_HIDE_DELAY_S.min}
                  max={AUTO_HIDE_DELAY_S.max}
                  step="0.1"
                  value={preferences.value.autoHideDelayMs / 1000}
                  onchange={(event) => setDelay(event, 'autoHideDelayMs', AUTO_HIDE_DELAY_S)}
                />
                <span class="unit">s</span>
              </span>
            </div>
            <div class="row delay">
              <div class="text">
                <div class="label">Show on music changes</div>
                <div class="hint">The mini island appears when a track starts, pauses or changes.</div>
              </div>
              <button
                class="switch"
                class:on={preferences.value.showOnMediaChange}
                role="switch"
                aria-checked={preferences.value.showOnMediaChange}
                aria-label="Show on music changes"
                onclick={() => toggle('showOnMediaChange')}
              ><span></span></button>
            </div>
          {/if}
        {/each}
      </div>
      <p class="note">In every mode, point at the top edge of the screen to open the island.</p>

      <h2>Opening</h2>
      <div class="card">
        <div class="row">
          <label class="text" for="open-delay">
            <span class="label">Hover delay</span>
            <span class="hint">
              How long to hold the pointer at the top edge before the island opens, from {OPEN_DELAY_S.min} to
              {OPEN_DELAY_S.max} seconds. A longer delay prevents accidental opens.
            </span>
          </label>
          <span class="field">
            <input
              id="open-delay"
              type="number"
              min={OPEN_DELAY_S.min}
              max={OPEN_DELAY_S.max}
              step="0.05"
              value={preferences.value.openDelayMs / 1000}
              onchange={(event) => setDelay(event, 'openDelayMs', OPEN_DELAY_S)}
            />
            <span class="unit">s</span>
          </span>
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

      <h2>In the island</h2>
      <div class="card" class:instant={reorder.instant} bind:this={layoutList}>
        {#each layout as entry, index (entry.id)}
          <div
            class="row layout-row"
            class:dragged={reorder.dragging === index}
            class:off={!entry.enabled}
            style:translate="0 {reorder.offset(index)}px"
          >
            <button
              class="grip"
              aria-label="Move {entry.name}. Use the up and down arrows."
              title="Drag to change the order"
              onpointerdown={(event) => reorder.start(event, index, layoutRows())}
              onpointermove={(event) => reorder.move(event)}
              onpointerup={(event) => reorder.end(event)}
              onpointercancel={(event) => reorder.end(event)}
              onkeydown={(event) => layoutKey(event, index)}
            ><DotsSixVertical size={18} weight="bold" /></button>
            <div class="text">
              <div class="label">{entry.name}</div>
              <div class="hint">{entry.description}</div>
            </div>
            <button
              class="switch"
              class:on={entry.enabled}
              role="switch"
              aria-checked={entry.enabled}
              aria-label="Show {entry.name}"
              onclick={() => toggleWidget(entry.id, !entry.enabled)}
            ><span></span></button>
          </div>
        {/each}
      </div>
      <p class="note">Drag to change the order. You can also arrange widgets in the island itself with the pencil button.</p>

      <h2>Built in</h2>
      <div class="card">
        {#each BUILT_IN as widget (widget.id)}
          <div class="row">
            <div class="text">
              <div class="label">{widget.name}</div>
              <div class="hint">{widget.description}</div>
            </div>
            <span class="badge">Built in</span>
          </div>
        {/each}
        {#each bundledWidgets as widget (widget.id)}
          <div class="row">
            <div class="text">
              <div class="label">{widget.name}</div>
              <div class="hint">{widget.description}</div>
            </div>
            <span class="badge">Built in</span>
          </div>
          {@render widgetSettingRows(widget)}
        {/each}
      </div>

      <h2>Add a widget</h2>
      <div class="card installer">
        <form class="row" onsubmit={installFromLink}>
          <span class="source-icon"><GithubLogo size={18} weight="fill" /></span>
          <input
            class="text-input link-input"
            type="text"
            placeholder="https://github.com/owner/widget"
            aria-label="Link to a widget on GitHub"
            spellcheck="false"
            autocomplete="off"
            bind:value={link}
            disabled={installer.busy !== null}
          />
          {#if !installer.pending}
            <button class="primary" type="submit" disabled={installer.busy !== null || !link.trim()}>Install</button>
          {/if}
        </form>
        <div class="row local">
          <span class="hint">Or from this computer. You can also drop it on this window.</span>
          <button class="secondary" onclick={() => installer.choose('zip')} disabled={installer.busy !== null}>
            <FileZip size={15} weight="fill" /> Zip archive…
          </button>
          <button class="secondary" onclick={() => installer.choose('folder')} disabled={installer.busy !== null}>
            <Folder size={15} weight="fill" /> Folder…
          </button>
        </div>
        {#if installer.busy === 'checking'}
          <div class="row progress">
            <span class="spin"><CircleNotch size={16} weight="bold" /></span> Getting the widget and checking it…
          </div>
        {/if}
        {#if installer.pending}
          {@const { manifest, origin, replaces } = installer.pending}
          <div class="row review">
            <div class="text">
              <div class="label">{manifest.name} <span class="version">{manifest.version}</span></div>
              {#if manifest.description}<div class="hint">{manifest.description}</div>{/if}
              <div class="hint">{manifest.author ? `By ${manifest.author} · ` : ''}{manifest.id}</div>
              <div class="hint origin">From {origin}</div>
              {#if replaces}
                <div class="hint">Replaces the installed version {replaces}. Its settings and data are kept.</div>
              {/if}
              {@render permissionChips(manifest)}
              <div class="hint">The widget runs in a sandbox and gets only what is listed above.</div>
            </div>
          </div>
          <div class="row review-actions">
            <button class="secondary" onclick={() => installer.cancel()} disabled={installer.busy !== null}>Cancel</button>
            <button class="primary" onclick={confirmInstall} disabled={installer.busy !== null}>
              {replaces ? 'Update' : 'Install'}
            </button>
          </div>
        {/if}
        {#if installer.error}<div class="row message bad">{installer.error}</div>{/if}
        {#if installer.done}<div class="row message ok">{installer.done}</div>{/if}
      </div>

      <h2>Installed</h2>
      {#if installedWidgets.length}
        <div class="card">
          {#each installedWidgets as widget (widget.id)}
            <div class="row">
              <div class="text">
                <div class="label">{widget.name} <span class="version">{widget.version}</span></div>
                <div class="hint">{widget.description || widget.id}</div>
                {@render permissionChips(widget)}
              </div>
              <span class="badge">{widget.source === 'dev' ? 'Development' : widget.author || 'Installed'}</span>
              {#if widget.source === 'user'}
                {#if removing === widget.id}
                  <span class="confirm">
                    <button class="link remove" onclick={() => uninstall(widget)}>Remove</button>
                    <button class="link" onclick={() => (removing = null)}>Keep</button>
                  </span>
                {:else}
                  <button class="icon-button" title="Remove" aria-label="Remove {widget.name}" onclick={() => (removing = widget.id)}>
                    <Trash size={16} />
                  </button>
                {/if}
              {/if}
            </div>
            {@render widgetSettingRows(widget)}
          {/each}
        </div>
      {:else}
        <div class="card empty">No widgets installed yet. Add one with a GitHub link, a zip archive or a folder.</div>
      {/if}
      {#if info.widgetsDir}<p class="path">{info.widgetsDir}</p>{/if}
    {:else if section === 'media'}
      <h1>Media</h1>
      <p class="intro">
        Apps that appear in the Windows media controls show up on the island by themselves. Some browsers, such as
        Yandex Browser, do not publish their media there; the Lizun browser extension brings their tabs in.
      </p>

      <h2>Browser extension</h2>
      <div class="card">
        <div class="row">
          <div class="text">
            <div class="label">Browser media</div>
            <div class="hint">Show music and videos that the Lizun extension finds in browser tabs.</div>
          </div>
          <button
            class="switch"
            class:on={preferences.value.browserMedia}
            role="switch"
            aria-checked={preferences.value.browserMedia}
            aria-label="Browser media"
            onclick={() => toggle('browserMedia')}
          ><span></span></button>
        </div>
        {#if preferences.value.browserMedia}
          <div class="row">
            <label class="text" for="browser-media-port">
              <span class="label">Port</span>
              <span class="hint">
                The extension connects here; its popup has to show the same port. From {PORTS.min} to {PORTS.max}.
              </span>
            </label>
            <span class="field">
              <input
                id="browser-media-port"
                class="port"
                type="number"
                min={PORTS.min}
                max={PORTS.max}
                step="1"
                value={preferences.value.browserMediaPort}
                onchange={setPort}
              />
            </span>
          </div>
          <div class="row">
            <div class="text">
              <div class="label">Status</div>
              <div class="hint status {browserMediaState.tone}">{browserMediaState.text}</div>
            </div>
          </div>
        {/if}
      </div>

      {#if preferences.value.browserMedia}
        <h2>Setup</h2>
        <div class="card">
          <ol class="steps">
            <li>Install the <b>Lizun</b> browser extension for Chrome, Yandex Browser, Edge, Opera or Firefox.</li>
            <li>Reload the tabs that were open before the extension was installed.</li>
            <li>
              If you changed the port, enter <b>{preferences.value.browserMediaPort}</b> in the extension's popup as
              well.
            </li>
          </ol>
        </div>
      {/if}
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

  {#if dropping}
    <div class="drop"><div>Drop a zip archive or a widget folder to install it</div></div>
  {/if}
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
    display: block;
    font-size: 14px;
    font-weight: 600;
  }

  .option {
    width: 100%;
    text-align: left;
  }

  .option:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .radio {
    flex: none;
    width: 18px;
    height: 18px;
    box-sizing: border-box;
    border-radius: 50%;
    border: 2px solid #4a4950;
    transition:
      border-color 150ms ease,
      border-width 150ms ease;
  }

  .radio.checked {
    border: 5.5px solid var(--accent);
    background: #fff;
  }

  .delay {
    padding-left: 50px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .field input {
    width: 64px;
    height: 28px;
    padding: 0 8px;
    box-sizing: border-box;
    border: 1px solid #3a3940;
    border-radius: 7px;
    background: #141316;
    color: var(--text);
    font: inherit;
    font-size: 13.5px;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .field input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .unit {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .note {
    margin: 10px 2px 0;
    font-size: 12.5px;
    color: var(--text-tertiary);
  }

  .version {
    margin-left: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-tertiary);
  }

  .hint {
    display: block;
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

  .intro {
    margin: -6px 2px 4px;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-secondary);
  }

  .field input.port {
    width: 76px;
  }

  .layout-row {
    position: relative;
    gap: 10px;
    padding-left: 8px;
    background: var(--card);
    transition: translate 220ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  .layout-row.dragged {
    z-index: 1;
    background: var(--card-raised);
    transition: none;
  }

  .instant .layout-row {
    transition: none;
  }

  .layout-row.off .label {
    color: var(--text-secondary);
  }

  .grip {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 32px;
    border-radius: 6px;
    color: var(--text-tertiary);
    cursor: grab;
    touch-action: none;
  }

  .grip:hover,
  .grip:focus-visible {
    color: var(--text);
    background: rgba(255, 255, 255, 0.05);
    outline: none;
  }

  .dragged .grip {
    cursor: grabbing;
  }

  /* A widget's own settings, listed under it. */
  .setting {
    padding-left: 32px;
  }

  .setting.reset {
    justify-content: flex-end;
    padding-top: 9px;
    padding-bottom: 9px;
  }

  .select,
  .text-input {
    height: 28px;
    padding: 0 8px;
    box-sizing: border-box;
    border: 1px solid #3a3940;
    border-radius: 7px;
    background: #141316;
    color: var(--text);
    font: inherit;
    font-size: 13.5px;
  }

  .select {
    min-width: 120px;
    max-width: 220px;
  }

  .text-input {
    width: 200px;
  }

  .select:focus,
  .text-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .link {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--accent);
  }

  .link:hover {
    text-decoration: underline;
  }

  .status.ok {
    color: var(--accent);
  }

  .status.bad {
    color: var(--danger);
  }

  .steps {
    margin: 0;
    padding: 14px 16px 14px 34px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-secondary);
  }

  .steps li + li {
    margin-top: 6px;
  }

  .steps b {
    color: var(--text);
    font-weight: 600;
  }

  .installer .row {
    gap: 10px;
  }

  .source-icon {
    display: grid;
    color: var(--text-secondary);
  }

  .text-input.link-input {
    flex: 1;
    height: 30px;
    width: auto;
    min-width: 0;
    user-select: text;
  }

  .local .hint {
    flex: 1;
    margin: 0;
  }

  .primary {
    flex: none;
    height: 30px;
    padding: 0 14px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 600;
    background: var(--accent);
    color: #fff;
  }

  .primary:hover {
    filter: brightness(1.1);
  }

  .primary:disabled,
  .secondary:disabled {
    opacity: 0.45;
    filter: none;
  }

  .progress {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .spin {
    display: grid;
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }

  .review,
  .row.review-actions {
    background: rgba(255, 255, 255, 0.02);
  }

  .review .hint + .hint,
  .review .chips + .hint {
    margin-top: 4px;
  }

  .review .chips {
    margin-bottom: 8px;
  }

  .origin {
    user-select: text;
    word-break: break-all;
  }

  .row.review-actions {
    justify-content: flex-end;
    border-top: none;
    padding-top: 0;
  }

  .message {
    font-size: 12.5px;
    line-height: 1.4;
    user-select: text;
  }

  .message.ok {
    color: var(--text-secondary);
  }

  .message.bad {
    color: var(--danger);
  }

  .icon-button {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    color: var(--text-tertiary);
  }

  .icon-button:hover {
    color: var(--danger);
    background: rgba(255, 255, 255, 0.05);
  }

  .confirm {
    flex: none;
    display: flex;
    gap: 12px;
  }

  .link.remove {
    color: var(--danger);
  }

  .drop {
    position: fixed;
    inset: 0;
    display: grid;
    padding: 24px;
    background: rgba(10, 10, 12, 0.72);
    pointer-events: none;
  }

  .drop div {
    display: grid;
    place-items: center;
    border: 2px dashed var(--accent);
    border-radius: 16px;
    font-size: 15px;
    font-weight: 600;
  }

  .error {
    margin-top: 14px;
    font-size: 12.5px;
    color: var(--danger);
  }
</style>
