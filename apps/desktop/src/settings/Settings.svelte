<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
  import { open } from '@tauri-apps/plugin-dialog';
  import ArrowClockwise from 'phosphor-svelte/lib/ArrowClockwise';
  import CircleNotch from 'phosphor-svelte/lib/CircleNotch';
  import ClipboardText from 'phosphor-svelte/lib/ClipboardText';
  import DotsSixVertical from 'phosphor-svelte/lib/DotsSixVertical';
  import FileZip from 'phosphor-svelte/lib/FileZip';
  import Folder from 'phosphor-svelte/lib/Folder';
  import FolderOpen from 'phosphor-svelte/lib/FolderOpen';
  import GearSix from 'phosphor-svelte/lib/GearSix';
  import GithubLogo from 'phosphor-svelte/lib/GithubLogo';
  import Info from 'phosphor-svelte/lib/Info';
  import Keyboard from 'phosphor-svelte/lib/Keyboard';
  import MusicNotes from 'phosphor-svelte/lib/MusicNotes';
  import PuzzlePiece from 'phosphor-svelte/lib/PuzzlePiece';
  import Trash from 'phosphor-svelte/lib/Trash';
  import X from 'phosphor-svelte/lib/X';
  import { browserMedia } from '../lib/browserMedia.svelte';
  import { clipboardHistory } from '../lib/clipboard.svelte';
  import {
    hotkeyStatus,
    modifiersOf,
    shortcutOfKey,
    shortcutOfMouse,
    shortcutParts,
    type Refusal,
  } from '../lib/hotkeys.svelte';
  import { i18n, LANGUAGES, t, tSplit, type MessageKey } from '../lib/i18n.svelte';
  import { invoke, isTauri, onFileDrag } from '../lib/platform';
  import { preferences, WIDGET_ROWS, type HotkeyAction, type IslandVisibility } from '../lib/preferences.svelte';
  import { Reorder } from '../lib/reorder.svelte';
  import { WidgetInstaller } from '../lib/widgets/installer.svelte';
  import {
    arrangeWidgets,
    BUILT_IN_WIDGETS,
    builtInTexts,
    setWidgetEnabled,
    widgetLayout,
    type LayoutEntry,
  } from '../lib/widgets/layout.svelte';
  import { localized, type SettingSpec, type SettingValue, type WidgetManifest } from '../lib/widgets/registry.svelte';
  import { widgetSettings, type SettingsOwner } from '../lib/widgets/settings.svelte';

  type Section = 'general' | 'shortcuts' | 'media' | 'widgets' | 'clipboard' | 'about';

  const VISIBILITY: IslandVisibility[] = ['always', 'autoHide', 'hidden'];

  const builtIn = $derived([
    ...BUILT_IN_WIDGETS.map((widget) => ({ id: widget.id, ...builtInTexts(widget.texts) })),
    { id: 'tray', ...builtInTexts('tray') },
  ]);

  const PERMISSIONS: Record<string, MessageKey> = {
    'media:read': 'settings.permissions.mediaRead',
    'media:control': 'settings.permissions.mediaControl',
    storage: 'settings.permissions.storage',
    activity: 'settings.permissions.activity',
  };

  let section = $state<Section>('general');
  let launchAtLogin = $state(false);
  let widgets = $state<WidgetManifest[]>([]);
  /** Settings of the built-in widgets, declared in the core in its current language. */
  let builtInSettings = $state<Record<string, SettingSpec[]>>({});
  // Widgets shipped with Lizun are listed with the other built-in ones.
  const shownWidgets = $derived(widgets.map(localized));
  const bundledWidgets = $derived(shownWidgets.filter((widget) => widget.source === 'bundled'));
  const installedWidgets = $derived(shownWidgets.filter((widget) => widget.source !== 'bundled'));
  let info = $state<{ version: string; widgetsDir: string | null }>({ version: '', widgetsDir: null });
  let error = $state<string | null>(null);

  onMount(() => {
    void preferences.start();
    void browserMedia.start();
    void clipboardHistory.start();
    void hotkeyStatus.start();
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
        else installer.error = t('settings.widgets.dropOne');
      },
    });
    return () => {
      installer.cancel();
      void stop.then((unlisten) => unlisten());
    };
  });

  // The core words the built-in widgets' settings, so they are read again in a new language.
  $effect(() => {
    void i18n.language;
    void invoke<Record<string, SettingSpec[]>>('widget_settings_built_in').then((settings) => (builtInSettings = settings ?? {}));
  });

  $effect(() => {
    document.title = t('settings.windowTitle');
  });

  /** "System (English)": the language the system would pick, in that language. */
  const systemLanguageName = $derived(LANGUAGES.find((language) => language.code === i18n.system)?.name ?? i18n.system);

  async function setLanguage(language: string): Promise<void> {
    error = null;
    try {
      await preferences.update({ language });
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  async function toggleLaunchAtLogin(): Promise<void> {
    error = null;
    try {
      if (launchAtLogin) await disable();
      else await enable();
      launchAtLogin = await isEnabled();
    } catch (err) {
      error = t('settings.errors.loginItem', { error: String(err) });
    }
  }

  async function setVisibility(value: IslandVisibility): Promise<void> {
    error = null;
    try {
      await preferences.update({ islandVisibility: value });
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
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
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  /** Allowed port range; it matches src-tauri/src/preferences.rs. */
  const PORTS = { min: 1024, max: 65535 };

  async function toggle(
    key: 'browserMedia' | 'showOnMediaChange' | 'clipboardHistory' | 'clipboardLimitEnabled' | ClipboardKind,
  ): Promise<void> {
    error = null;
    try {
      await preferences.update({ [key]: !preferences.value[key] });
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  async function setPort(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const port = Number.parseInt(input.value, 10);
    if (!Number.isInteger(port) || port < PORTS.min || port > PORTS.max) {
      input.value = String(preferences.value.browserMediaPort);
      error = t('settings.errors.port', PORTS);
      return;
    }
    error = null;
    try {
      await preferences.update({ browserMediaPort: port });
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  /** Lifetimes offered for the clipboard history, in hours; 0 keeps entries for good. They
   * fit the range in src-tauri/src/preferences.rs. */
  const LIFETIMES = [
    { hours: 1, label: 'hour' },
    { hours: 24, label: 'day' },
    { hours: 168, label: 'week' },
    { hours: 720, label: 'month' },
    { hours: 0, label: 'forever' },
  ] as const;
  /** Kinds of clips the history can keep, each with its own switch. */
  const CLIPBOARD_KINDS = [
    { key: 'clipboardText', label: 'text' },
    { key: 'clipboardPictures', label: 'pictures' },
    { key: 'clipboardFiles', label: 'files' },
  ] as const;
  type ClipboardKind = (typeof CLIPBOARD_KINDS)[number]['key'];
  /** The last kind that is on cannot be turned off; the history switch does that. */
  const lastKind = $derived(CLIPBOARD_KINDS.filter((kind) => preferences.value[kind.key]).length === 1);
  /** Allowed limit range; it matches src-tauri/src/preferences.rs. */
  const CLIPBOARD_LIMIT = { min: 1, max: 1000 };
  let confirmClear = $state(false);

  async function setLifetime(hours: number): Promise<void> {
    error = null;
    try {
      await preferences.update({ clipboardLifetimeHours: hours });
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  async function setLimit(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const limit = Number.parseInt(input.value, 10);
    if (!Number.isInteger(limit) || limit < CLIPBOARD_LIMIT.min || limit > CLIPBOARD_LIMIT.max) {
      input.value = String(preferences.value.clipboardLimit);
      error = t('settings.errors.limit', CLIPBOARD_LIMIT);
      return;
    }
    error = null;
    try {
      await preferences.update({ clipboardLimit: limit });
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  /** Global shortcuts, grouped as the settings show them. */
  const SHORTCUT_GROUPS: { title: 'island' | 'music'; actions: HotkeyAction[] }[] = [
    { title: 'island', actions: ['toggleIsland'] },
    { title: 'music', actions: ['playPause', 'nextTrack', 'previousTrack'] },
  ];
  const HOTKEY_ACTIONS = SHORTCUT_GROUPS.flatMap((group) => group.actions);

  /** The action whose shortcut is being recorded. Set shortcuts are off meanwhile, so pressing
   * one is recorded instead of firing. */
  let recording = $state<HotkeyAction | null>(null);
  /** Modifiers held while recording, shown before the key comes. */
  let heldModifiers = $state<string[]>([]);
  let shortcutError = $state<{ action: HotkeyAction; text: string } | null>(null);

  function record(action: HotkeyAction): void {
    if (recording === action) {
      stopRecording();
      return;
    }
    if (!recording) void invoke('hotkeys_pause', { paused: true });
    recording = action;
    heldModifiers = [];
    shortcutError = null;
  }

  function stopRecording(): void {
    if (!recording) return;
    recording = null;
    heldModifiers = [];
    void invoke('hotkeys_pause', { paused: false });
  }

  /** Takes a pressed shortcut if the core allows it and no other action has it; otherwise says
   * why and keeps recording, so the next try can follow at once. */
  async function useShortcut(action: HotkeyAction, shortcut: string): Promise<void> {
    let written: string;
    try {
      written = await invoke<string>('hotkeys_check', { shortcut });
    } catch (refusal) {
      shortcutError = { action, text: t(`settings.shortcuts.refused.${refusal as Refusal}`) };
      return;
    }
    const other = HOTKEY_ACTIONS.find((other) => other !== action && preferences.value.hotkeys[other] === written);
    if (other) {
      shortcutError = { action, text: t('settings.shortcuts.inUse', { action: t(`settings.shortcuts.${other}`) }) };
      return;
    }
    stopRecording();
    shortcutError = null;
    await setHotkey(action, written);
  }

  async function setHotkey(action: HotkeyAction, shortcut: string | null): Promise<void> {
    error = null;
    try {
      await preferences.update({ hotkeys: { ...preferences.value.hotkeys, [action]: shortcut } });
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  function recordKey(event: KeyboardEvent): void {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;
    const shortcut = shortcutOfKey(event);
    if (!shortcut) heldModifiers = modifiersOf(event);
    else if (shortcut === 'Escape') stopRecording();
    else void useShortcut(recording, shortcut);
  }

  function releaseKey(event: KeyboardEvent): void {
    if (recording) heldModifiers = modifiersOf(event);
  }

  function recordMouse(event: MouseEvent): void {
    if (!recording) return;
    const shortcut = shortcutOfMouse(event);
    if (shortcut) {
      event.preventDefault();
      void useShortcut(recording, shortcut);
    } else if (!(event.target as Element).closest('.recorder.recording')) {
      // A click anywhere else ends recording; on the recorder itself its own click does.
      stopRecording();
    }
  }

  /** The side buttons would also go back and forward in the window's history. */
  function holdBackSideButton(event: MouseEvent): void {
    if (recording && shortcutOfMouse(event)) event.preventDefault();
  }

  $effect(() => {
    if (section !== 'shortcuts') untrack(stopRecording);
  });

  onDestroy(stopRecording);

  const browserMediaState = $derived.by((): { text: string; tone: 'ok' | 'wait' | 'bad' } => {
    const status = browserMedia.status;
    if (!status) return { text: t('settings.media.starting'), tone: 'wait' };
    if (status.error) return { text: t('settings.media.retrying', { error: status.error }), tone: 'bad' };
    if (status.browsers.length) {
      return { text: t('settings.media.connected', { browsers: [...new Set(status.browsers)].join(', ') }), tone: 'ok' };
    }
    if (status.listening) return { text: t('settings.media.waiting', { port: status.port }), tone: 'wait' };
    return { text: t('settings.media.starting'), tone: 'wait' };
  });

  async function reloadWidgets(): Promise<void> {
    widgets = (await invoke<WidgetManifest[]>('widgets_reload')) ?? [];
    await widgetSettings.load();
  }

  async function setWidgetSetting(widget: SettingsOwner, key: string, value: SettingValue): Promise<void> {
    error = null;
    try {
      await widgetSettings.set(widget.id, key, value);
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
    }
  }

  /** Numbers are clamped to the declared range; anything that is not a number restores the saved value. */
  async function setWidgetNumber(
    event: Event,
    widget: SettingsOwner,
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

  /** Fills a text setting with the path of a file the user picks. */
  async function chooseFile(
    widget: SettingsOwner,
    setting: Extract<SettingSpec, { type: 'text' }>,
    file: { name: string; extensions: string[] },
  ): Promise<void> {
    error = null;
    try {
      const path = isTauri
        ? await open({
            title: t('settings.widgets.chooseFileFor', { label: setting.label }),
            filters: [{ name: file.name, extensions: file.extensions }],
          })
        : `C:\\Users\\you\\Downloads\\calendar.${file.extensions[0]}`;
      if (typeof path === 'string') await setWidgetSetting(widget, setting.key, path);
    } catch (err) {
      error = t('settings.errors.fileDialog', { error: String(err) });
    }
  }

  async function resetWidgetSettings(widget: SettingsOwner): Promise<void> {
    error = null;
    try {
      await widgetSettings.reset(widget.id);
    } catch (err) {
      error = t('settings.errors.reset', { error: String(err) });
    }
  }

  const layout = $derived(widgetLayout(widgets, preferences.value.widgets));
  /** The widgets row by row, with a heading in front of every row after the first: a widget
   * dragged past a heading goes into that row. */
  const layoutItems = $derived(
    Array.from({ length: WIDGET_ROWS }, (_, row) => [
      ...(row ? [{ heading: row }] : []),
      ...layout.filter((entry) => entry.row === row),
    ]).flat(),
  );
  let layoutList = $state<HTMLElement>();
  const reorder = new Reorder('y', 0, (from, to) => void moveLayout(from, to));

  function layoutRows(): HTMLElement[] {
    return [...(layoutList?.querySelectorAll<HTMLElement>('.layout-row') ?? [])];
  }

  async function moveLayout(from: number, to: number, refocus = false): Promise<void> {
    error = null;
    const items = [...layoutItems];
    items.splice(to, 0, ...items.splice(from, 1));
    const rows: LayoutEntry[][] = [[]];
    for (const item of items) {
      if ('heading' in item) rows.push([]);
      else rows[rows.length - 1].push(item);
    }
    try {
      await arrangeWidgets(rows);
    } catch (err) {
      error = t('settings.errors.saveOrder', { error: String(err) });
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
    if (to >= 0 && to < layoutItems.length) void moveLayout(index, to, true);
  }

  async function toggleWidget(id: string, enabled: boolean): Promise<void> {
    error = null;
    try {
      await setWidgetEnabled(layout, id, enabled);
    } catch (err) {
      error = t('settings.errors.save', { error: String(err) });
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
      error = t('settings.errors.remove', { name: widget.name, error: String(err) });
    }
  }

  function openWidgetsFolder(): void {
    error = null;
    invoke('widgets_open_folder').catch((err) => (error = t('settings.errors.openFolder', { error: String(err) })));
  }
</script>

{#snippet widgetSettingRows(widget: SettingsOwner)}
  {#if widget.settings.length}
    {@const values = widgetSettings.of(widget)}
    {#each widget.settings as setting (setting.key)}
      {@render settingRow(widget, setting, values[setting.key])}
    {/each}
    <div class="row setting reset">
      <button class="link" onclick={() => resetWidgetSettings(widget)}>{t('settings.widgets.resetDefaults')}</button>
    </div>
  {/if}
{/snippet}

{#snippet permissionChips(widget: WidgetManifest)}
  <!-- A manifest may list a permission or origin twice; each is shown once. -->
  <div class="chips">
    {#each new Set(widget.permissions) as permission (permission)}
      <span class="chip">{permission in PERMISSIONS ? t(PERMISSIONS[permission]) : permission}</span>
    {/each}
    {#each new Set(widget.network) as origin (origin)}
      <span class="chip">{t('settings.permissions.network', { origin })}</span>
    {/each}
    {#if !widget.permissions.length && !widget.network.length}
      <span class="chip">{t('settings.permissions.none')}</span>
    {/if}
  </div>
{/snippet}

{#snippet settingRow(widget: SettingsOwner, setting: SettingSpec, value: SettingValue)}
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
      <span class="picker">
        <input
          {id}
          class="text-input"
          type="text"
          maxlength={setting.maxLength}
          placeholder={setting.placeholder}
          title={setting.file ? String(value) : undefined}
          {value}
          onchange={(event) => setWidgetSetting(widget, setting.key, event.currentTarget.value)}
        />
        {#if setting.file}
          {@const file = setting.file}
          <button class="secondary compact" title={t('settings.widgets.chooseFile')} onclick={() => chooseFile(widget, setting, file)}>
            <FolderOpen size={14} weight="fill" /> {t('settings.widgets.choose')}
          </button>
        {/if}
      </span>
    {/if}
  </div>
{/snippet}

<svelte:window
  onkeydowncapture={recordKey}
  onkeyupcapture={releaseKey}
  onmousedowncapture={recordMouse}
  onmouseupcapture={holdBackSideButton}
  onauxclick={holdBackSideButton}
  onblur={stopRecording}
/>

<div class="window">
  <nav>
    <div class="brand"><span class="logo"></span>Lizun</div>
    <button class:active={section === 'general'} onclick={() => (section = 'general')}>
      <GearSix size={16} weight="fill" /> {t('settings.nav.general')}
    </button>
    <button class:active={section === 'shortcuts'} onclick={() => (section = 'shortcuts')}>
      <Keyboard size={16} weight="fill" /> {t('settings.nav.shortcuts')}
    </button>
    <button class:active={section === 'media'} onclick={() => (section = 'media')}>
      <MusicNotes size={16} weight="fill" /> {t('settings.nav.media')}
    </button>
    <button class:active={section === 'widgets'} onclick={() => (section = 'widgets')}>
      <PuzzlePiece size={16} weight="fill" /> {t('settings.nav.widgets')}
    </button>
    <button class:active={section === 'clipboard'} onclick={() => (section = 'clipboard')}>
      <ClipboardText size={16} weight="fill" /> {t('settings.nav.clipboard')}
    </button>
    <button class:active={section === 'about'} onclick={() => (section = 'about')}>
      <Info size={16} weight="fill" /> {t('settings.nav.about')}
    </button>
  </nav>

  <main>
    {#if section === 'general'}
      <h1>{t('settings.general.title')}</h1>
      <div class="card">
        <div class="row">
          <label class="text" for="language">
            <span class="label">{t('settings.general.language')}</span>
            <span class="hint">{t('settings.general.languageHint')}</span>
          </label>
          <select
            id="language"
            class="select"
            value={preferences.value.language}
            onchange={(event) => setLanguage(event.currentTarget.value)}
          >
            <option value="system">{t('settings.general.systemLanguage', { name: systemLanguageName })}</option>
            {#each LANGUAGES as language (language.code)}
              <option value={language.code} lang={language.code}>{language.name}</option>
            {/each}
          </select>
        </div>
        <div class="row">
          <div class="text">
            <div class="label">{t('settings.general.launchAtLogin')}</div>
            <div class="hint">{t('settings.general.launchAtLoginHint')}</div>
          </div>
          <button
            class="switch"
            class:on={launchAtLogin}
            role="switch"
            aria-checked={launchAtLogin}
            aria-label={t('settings.general.launchAtLogin')}
            onclick={toggleLaunchAtLogin}
          ><span></span></button>
        </div>
      </div>

      <h2>{t('settings.general.miniIsland')}</h2>
      <div class="card" role="radiogroup" aria-label={t('settings.general.miniIsland')}>
        {#each VISIBILITY as option (option)}
          {@const checked = preferences.value.islandVisibility === option}
          <button class="row option" role="radio" aria-checked={checked} onclick={() => setVisibility(option)}>
            <span class="radio" class:checked></span>
            <span class="text">
              <span class="label">{t(`settings.general.${option}`)}</span>
              <span class="hint">{t(`settings.general.${option}Hint`)}</span>
            </span>
          </button>
          {#if option === 'autoHide' && checked}
            <div class="row delay">
              <label class="text" for="auto-hide-delay">
                <span class="label">{t('settings.general.delay')}</span>
                <span class="hint">{t('settings.general.delayHint', AUTO_HIDE_DELAY_S)}</span>
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
                <span class="unit">{t('settings.seconds')}</span>
              </span>
            </div>
            <div class="row delay">
              <div class="text">
                <div class="label">{t('settings.general.showOnMusic')}</div>
                <div class="hint">{t('settings.general.showOnMusicHint')}</div>
              </div>
              <button
                class="switch"
                class:on={preferences.value.showOnMediaChange}
                role="switch"
                aria-checked={preferences.value.showOnMediaChange}
                aria-label={t('settings.general.showOnMusic')}
                onclick={() => toggle('showOnMediaChange')}
              ><span></span></button>
            </div>
          {/if}
        {/each}
      </div>
      <p class="note">{t('settings.general.edgeNote')}</p>

      <h2>{t('settings.general.opening')}</h2>
      <div class="card">
        <div class="row">
          <label class="text" for="open-delay">
            <span class="label">{t('settings.general.hoverDelay')}</span>
            <span class="hint">{t('settings.general.hoverDelayHint', OPEN_DELAY_S)}</span>
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
            <span class="unit">{t('settings.seconds')}</span>
          </span>
        </div>
      </div>
    {:else if section === 'shortcuts'}
      <h1>{t('settings.shortcuts.title')}</h1>
      <p class="intro">{t('settings.shortcuts.intro')}</p>
      {#each SHORTCUT_GROUPS as group (group.title)}
        <h2>{t(`settings.shortcuts.${group.title}`)}</h2>
        <div class="card">
          {#each group.actions as action (action)}
            {@const shortcut = preferences.value.hotkeys[action]}
            {@const active = recording === action}
            {@const refused = shortcutError?.action === action ? shortcutError.text : null}
            <div class="row">
              <div class="text">
                <div class="label">{t(`settings.shortcuts.${action}`)}</div>
                {#if refused}
                  <div class="hint status bad">{refused}</div>
                {:else if active}
                  <div class="hint">{t('settings.shortcuts.recordingHint')}</div>
                {:else if shortcut && hotkeyStatus.taken.includes(action)}
                  <div class="hint status bad">{t('settings.shortcuts.taken')}</div>
                {:else if action === 'toggleIsland'}
                  <div class="hint">{t('settings.shortcuts.toggleIslandHint')}</div>
                {/if}
              </div>
              <span class="shortcut">
                <button
                  class="recorder"
                  class:recording={active}
                  aria-label={t('settings.shortcuts.change', { action: t(`settings.shortcuts.${action}`) })}
                  onclick={() => record(action)}
                >
                  {#if active}
                    {#each heldModifiers as part (part)}<kbd>{part}</kbd>{/each}
                    <span class="placeholder">{heldModifiers.length ? '…' : t('settings.shortcuts.press')}</span>
                  {:else if shortcut}
                    {#each shortcutParts(shortcut) as part (part)}<kbd>{part}</kbd>{/each}
                  {:else}
                    <span class="placeholder">{t('settings.shortcuts.notSet')}</span>
                  {/if}
                </button>
                <button
                  class="icon-button"
                  title={t('settings.shortcuts.turnOff')}
                  aria-label={t('settings.shortcuts.turnOff')}
                  disabled={!shortcut || active}
                  onclick={() => setHotkey(action, null)}
                ><X size={14} weight="bold" /></button>
              </span>
            </div>
          {/each}
        </div>
      {/each}
      <p class="note">{t('settings.shortcuts.note')}</p>
    {:else if section === 'widgets'}
      <div class="head">
        <h1>{t('settings.widgets.title')}</h1>
        <div class="actions">
          <button class="secondary" onclick={openWidgetsFolder}>
            <FolderOpen size={15} weight="fill" /> {t('settings.widgets.openFolder')}
          </button>
          <button class="secondary" onclick={reloadWidgets}>
            <ArrowClockwise size={15} weight="bold" /> {t('settings.widgets.reload')}
          </button>
        </div>
      </div>

      <h2>{t('settings.widgets.inIsland')}</h2>
      <div class="card" class:instant={reorder.instant} bind:this={layoutList}>
        {#each layoutItems as item, index ('heading' in item ? `row-${item.heading}` : item.id)}
          {#if 'heading' in item}
            <div class="row layout-row heading" style:translate="0 {reorder.offset(index)}px">
              <div class="text">
                <div class="label">{t('settings.widgets.rowHeading', { number: item.heading + 1 })}</div>
                <div class="hint">{t('settings.widgets.rowHeadingHint')}</div>
              </div>
            </div>
          {:else}
            {@const entry = item}
            <div
              class="row layout-row"
              class:dragged={reorder.dragging === index}
              class:off={!entry.enabled}
              style:translate="0 {reorder.offset(index)}px"
            >
              <button
                class="grip"
                aria-label={t('settings.widgets.moveHint', { name: entry.name })}
                title={t('settings.widgets.dragToReorder')}
                onpointerdown={(event) => reorder.start(event, index, layoutRows())}
                onpointermove={(event) => reorder.move(event)}
                onpointerup={(event) => reorder.end(event)}
                onpointercancel={(event) => reorder.end(event)}
                onlostpointercapture={(event) => reorder.end(event)}
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
                aria-label={t('settings.widgets.show', { name: entry.name })}
                onclick={() => toggleWidget(entry.id, !entry.enabled)}
              ><span></span></button>
            </div>
          {/if}
        {/each}
      </div>
      <p class="note">{t('settings.widgets.orderNote')}</p>

      <h2>{t('settings.widgets.builtIn')}</h2>
      <div class="card">
        {#each builtIn as widget (widget.id)}
          <div class="row">
            <div class="text">
              <div class="label">{widget.name}</div>
              <div class="hint">{widget.description}</div>
            </div>
            <span class="badge">{t('settings.widgets.builtIn')}</span>
          </div>
          {@render widgetSettingRows({ id: widget.id, settings: builtInSettings[widget.id] ?? [] })}
        {/each}
        {#each bundledWidgets as widget (widget.id)}
          <div class="row">
            <div class="text">
              <div class="label">{widget.name}</div>
              <div class="hint">{widget.description}</div>
            </div>
            <span class="badge">{t('settings.widgets.builtIn')}</span>
          </div>
          {@render widgetSettingRows(widget)}
        {/each}
      </div>

      <h2>{t('settings.widgets.add')}</h2>
      <div class="card installer">
        <form class="row" onsubmit={installFromLink}>
          <span class="source-icon"><GithubLogo size={18} weight="fill" /></span>
          <input
            class="text-input link-input"
            type="text"
            placeholder="https://github.com/owner/widget"
            aria-label={t('settings.widgets.githubLink')}
            spellcheck="false"
            autocomplete="off"
            bind:value={link}
            disabled={installer.busy !== null}
          />
          {#if !installer.pending}
            <button class="primary" type="submit" disabled={installer.busy !== null || !link.trim()}>
              {t('settings.widgets.install')}
            </button>
          {/if}
        </form>
        <div class="row local">
          <span class="hint">{t('settings.widgets.fromComputer')}</span>
          <button class="secondary" onclick={() => installer.choose('zip')} disabled={installer.busy !== null}>
            <FileZip size={15} weight="fill" /> {t('settings.widgets.zip')}
          </button>
          <button class="secondary" onclick={() => installer.choose('folder')} disabled={installer.busy !== null}>
            <Folder size={15} weight="fill" /> {t('settings.widgets.folder')}
          </button>
        </div>
        {#if installer.busy === 'checking'}
          <div class="row progress">
            <span class="spin"><CircleNotch size={16} weight="bold" /></span> {t('settings.widgets.checking')}
          </div>
        {/if}
        {#if installer.pending}
          {@const { origin, replaces, previousSource, sourceChanged } = installer.pending}
          {@const manifest = localized(installer.pending.manifest)}
          <div class="row review">
            <div class="text">
              <div class="label">{manifest.name} <span class="version">{manifest.version}</span></div>
              {#if manifest.description}<div class="hint">{manifest.description}</div>{/if}
              <div class="hint">
                {manifest.author ? `${t('settings.widgets.by', { author: manifest.author })} · ` : ''}{manifest.id}
              </div>
              <div class="hint origin">{t('settings.widgets.from', { origin })}</div>
              {#if sourceChanged}
                {#if replaces}
                  <div class="hint">{t('settings.widgets.replacesVersion', { version: replaces })}</div>
                {/if}
                <div class="hint warning">
                  {previousSource
                    ? t('settings.widgets.sourceChanged', {
                        source: previousSource === 'local' ? t('settings.widgets.sourceLocal') : previousSource,
                      })
                    : t('settings.widgets.sourceUnknown')}
                </div>
              {:else if replaces}
                <div class="hint">{t('settings.widgets.replaces', { version: replaces })}</div>
              {/if}
              {@render permissionChips(manifest)}
              <div class="hint">{t('settings.widgets.sandbox')}</div>
            </div>
          </div>
          <div class="row review-actions">
            <button class="secondary" onclick={() => installer.cancel()} disabled={installer.busy !== null}>
              {t('settings.widgets.cancel')}
            </button>
            <button class="primary" onclick={confirmInstall} disabled={installer.busy !== null}>
              {replaces ? t('settings.widgets.update') : t('settings.widgets.install')}
            </button>
          </div>
        {/if}
        {#if installer.error}<div class="row message bad">{installer.error}</div>{/if}
        {#if installer.done}<div class="row message ok">{installer.done}</div>{/if}
      </div>

      <h2>{t('settings.widgets.installed')}</h2>
      {#if installedWidgets.length}
        <div class="card">
          {#each installedWidgets as widget (widget.id)}
            <div class="row">
              <div class="text">
                <div class="label">{widget.name} <span class="version">{widget.version}</span></div>
                <div class="hint">{widget.description || widget.id}</div>
                {@render permissionChips(widget)}
              </div>
              <span class="badge">
                {widget.source === 'dev' ? t('settings.widgets.development') : widget.author || t('settings.widgets.installed')}
              </span>
              {#if widget.source === 'user'}
                {#if removing === widget.id}
                  <span class="confirm">
                    <button class="link remove" onclick={() => uninstall(widget)}>{t('settings.widgets.remove')}</button>
                    <button class="link" onclick={() => (removing = null)}>{t('settings.widgets.keep')}</button>
                  </span>
                {:else}
                  <button
                    class="icon-button"
                    title={t('settings.widgets.remove')}
                    aria-label={t('settings.widgets.removeNamed', { name: widget.name })}
                    onclick={() => (removing = widget.id)}
                  >
                    <Trash size={16} />
                  </button>
                {/if}
              {/if}
            </div>
            {@render widgetSettingRows(widget)}
          {/each}
        </div>
      {:else}
        <div class="card empty">{t('settings.widgets.none')}</div>
      {/if}
      {#if info.widgetsDir}<p class="path">{info.widgetsDir}</p>{/if}
    {:else if section === 'media'}
      <h1>{t('settings.media.title')}</h1>
      <p class="intro">{t('settings.media.intro')}</p>

      <h2>{t('settings.media.extension')}</h2>
      <div class="card">
        <div class="row">
          <div class="text">
            <div class="label">{t('settings.media.browserMedia')}</div>
            <div class="hint">{t('settings.media.browserMediaHint')}</div>
          </div>
          <button
            class="switch"
            class:on={preferences.value.browserMedia}
            role="switch"
            aria-checked={preferences.value.browserMedia}
            aria-label={t('settings.media.browserMedia')}
            onclick={() => toggle('browserMedia')}
          ><span></span></button>
        </div>
        {#if preferences.value.browserMedia}
          <div class="row">
            <label class="text" for="browser-media-port">
              <span class="label">{t('settings.media.port')}</span>
              <span class="hint">{t('settings.media.portHint', PORTS)}</span>
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
              <div class="label">{t('settings.media.status')}</div>
              <div class="hint status {browserMediaState.tone}">{browserMediaState.text}</div>
            </div>
          </div>
        {/if}
      </div>

      {#if preferences.value.browserMedia}
        {@const [installBefore, installAfter] = tSplit('settings.media.installExtension', 'name')}
        {@const [portBefore, portAfter] = tSplit('settings.media.samePort', 'port')}
        <h2>{t('settings.media.setup')}</h2>
        <div class="card">
          <ol class="steps">
            <li>{installBefore}<b>Lizun</b>{installAfter}</li>
            <li>{t('settings.media.reloadTabs')}</li>
            <li>{portBefore}<b>{preferences.value.browserMediaPort}</b>{portAfter}</li>
          </ol>
        </div>
      {/if}
    {:else if section === 'clipboard'}
      {@const lifetime = preferences.value.clipboardLifetimeHours}
      <h1>{t('settings.clipboard.title')}</h1>
      <p class="intro">{t('settings.clipboard.intro')}</p>
      <div class="card">
        <div class="row">
          <div class="text">
            <div class="label">{t('settings.clipboard.history')}</div>
            <div class="hint">{t('settings.clipboard.historyHint')}</div>
          </div>
          <button
            class="switch"
            class:on={preferences.value.clipboardHistory}
            role="switch"
            aria-checked={preferences.value.clipboardHistory}
            aria-label={t('settings.clipboard.history')}
            onclick={() => toggle('clipboardHistory')}
          ><span></span></button>
        </div>
      </div>
      {#if preferences.value.clipboardHistory}
        <h2>{t('settings.clipboard.kinds')}</h2>
        <div class="card">
          {#each CLIPBOARD_KINDS as kind (kind.key)}
            {@const on = preferences.value[kind.key]}
            <div class="row">
              <div class="text">
                <div class="label">{t(`settings.clipboard.${kind.label}`)}</div>
                <div class="hint">{t(`settings.clipboard.${kind.label}Hint`)}</div>
              </div>
              <button
                class="switch"
                class:on
                role="switch"
                aria-checked={on}
                aria-label={t(`settings.clipboard.${kind.label}`)}
                disabled={on && lastKind}
                onclick={() => toggle(kind.key)}
              ><span></span></button>
            </div>
          {/each}
        </div>
        <p class="note">{t('settings.clipboard.kindsNote')}</p>

        <h2>{t('settings.clipboard.storage')}</h2>
        <div class="card">
          <div class="row">
            <label class="text" for="clipboard-lifetime">
              <span class="label">{t('settings.clipboard.keep')}</span>
              <span class="hint">{t('settings.clipboard.keepHint')}</span>
            </label>
            <select
              id="clipboard-lifetime"
              class="select"
              value={lifetime}
              onchange={(event) => setLifetime(Number(event.currentTarget.value))}
            >
              {#each LIFETIMES as option (option.hours)}
                <option value={option.hours}>{t(`settings.clipboard.${option.label}`)}</option>
              {/each}
              <!-- A value set by hand in preferences.json is shown as it is. -->
              {#if !LIFETIMES.some((option) => option.hours === lifetime)}
                <option value={lifetime}>{t('settings.clipboard.hours', { count: lifetime })}</option>
              {/if}
            </select>
          </div>
          <div class="row">
            <div class="text">
              <div class="label">{t('settings.clipboard.limit')}</div>
              <div class="hint">{t('settings.clipboard.limitHint')}</div>
            </div>
            <button
              class="switch"
              class:on={preferences.value.clipboardLimitEnabled}
              role="switch"
              aria-checked={preferences.value.clipboardLimitEnabled}
              aria-label={t('settings.clipboard.limit')}
              onclick={() => toggle('clipboardLimitEnabled')}
            ><span></span></button>
          </div>
          {#if preferences.value.clipboardLimitEnabled}
            <div class="row">
              <label class="text" for="clipboard-limit">
                <span class="label">{t('settings.clipboard.maxEntries')}</span>
                <span class="hint">{t('settings.clipboard.maxEntriesHint', CLIPBOARD_LIMIT)}</span>
              </label>
              <span class="field">
                <input
                  id="clipboard-limit"
                  class="port"
                  type="number"
                  min={CLIPBOARD_LIMIT.min}
                  max={CLIPBOARD_LIMIT.max}
                  step="1"
                  value={preferences.value.clipboardLimit}
                  onchange={setLimit}
                />
              </span>
            </div>
          {/if}
          <div class="row">
            <div class="text">
              <div class="label">{t('settings.clipboard.saved')}</div>
              <div class="hint">{t('settings.clipboard.entries', { count: clipboardHistory.entries.length })}</div>
            </div>
            {#if confirmClear}
              <span class="confirm">
                <span class="hint">{t('settings.clipboard.clearConfirm')}</span>
                <button
                  class="link remove"
                  onclick={() => {
                    clipboardHistory.clear();
                    confirmClear = false;
                  }}>{t('settings.clipboard.delete')}</button
                >
                <button class="link" onclick={() => (confirmClear = false)}>{t('settings.clipboard.keepIt')}</button>
              </span>
            {:else}
              <button class="secondary" disabled={!clipboardHistory.entries.length} onclick={() => (confirmClear = true)}>
                <Trash size={15} weight="fill" /> {t('settings.clipboard.clear')}
              </button>
            {/if}
          </div>
        </div>
      {/if}
      <p class="note">{t('settings.clipboard.private')}</p>
    {:else}
      <h1>{t('settings.about.title')}</h1>
      <div class="card about">
        <span class="logo big"></span>
        <div class="text">
          <div class="label">Lizun {info.version}</div>
          <div class="hint">{t('settings.about.tagline')}</div>
        </div>
      </div>
      <button class="danger" onclick={() => invoke('app_quit')}>{t('settings.about.quit')}</button>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}
  </main>

  {#if dropping}
    <div class="drop"><div>{t('settings.widgets.drop')}</div></div>
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
    text-align: left;
    white-space: nowrap;
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

  .switch:disabled {
    opacity: 0.45;
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

  /* Lines up with the widget names, which follow the grip. */
  .layout-row.heading {
    padding-top: 9px;
    padding-bottom: 9px;
    padding-left: 42px;
    background: rgba(255, 255, 255, 0.025);
  }

  .layout-row.heading .label {
    font-size: 12px;
    font-weight: 600;
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

  .picker {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .secondary.compact {
    height: 28px;
    padding: 0 10px;
    gap: 5px;
    font-size: 12.5px;
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

  .review .warning {
    color: var(--danger);
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

  .shortcut {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .recorder {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    min-width: 150px;
    height: 30px;
    padding: 0 6px 0 10px;
    box-sizing: border-box;
    border: 1px solid #3a3940;
    border-radius: 8px;
    background: #141316;
  }

  .recorder:hover {
    border-color: #4a4950;
  }

  .recorder.recording {
    border-color: var(--accent);
  }

  .recorder kbd {
    padding: 2px 7px;
    border-radius: 5px;
    background: var(--card-raised);
    box-shadow: inset 0 -1px 0 rgba(0, 0, 0, 0.35);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }

  .recorder .placeholder {
    padding-right: 4px;
    font-size: 12.5px;
    color: var(--text-tertiary);
  }

  .recorder.recording .placeholder {
    color: var(--accent);
  }

  .icon-button:disabled {
    visibility: hidden;
  }

  .confirm {
    flex: none;
    display: flex;
    align-items: center;
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
