<script lang="ts">
  import ClipboardText from 'phosphor-svelte/lib/ClipboardText';
  import GearSix from 'phosphor-svelte/lib/GearSix';
  import PencilSimple from 'phosphor-svelte/lib/PencilSimple';
  import SquaresFour from 'phosphor-svelte/lib/SquaresFour';
  import Trash from 'phosphor-svelte/lib/Trash';
  import Tray from 'phosphor-svelte/lib/Tray';
  import { clipboardHistory } from '../lib/clipboard.svelte';
  import { t } from '../lib/i18n.svelte';
  import { invoke } from '../lib/platform';
  import { preferences } from '../lib/preferences.svelte';
  import { ui, type PanelTab } from '../lib/ui.svelte';
  import ClipboardView from './ClipboardView.svelte';
  import TrayView from './TrayView.svelte';
  import WidgetsView from './WidgetsView.svelte';

  let { tab = $bindable('widgets') }: { tab: PanelTab } = $props();

  // The tray takes the width of the widget row, so switching tabs does not shrink the island
  // out from under the cursor.
  let widgetsWidth = $state(0);
  let editing = $state(false);

  /** Clearing the clipboard history takes a second click within a few seconds. */
  let clearArmed = $state(false);
  let clearTimer: ReturnType<typeof setTimeout> | undefined;

  function clearHistory(): void {
    clearTimeout(clearTimer);
    if (clearArmed) {
      clearArmed = false;
      clipboardHistory.clear();
      return;
    }
    clearArmed = true;
    clearTimer = setTimeout(() => (clearArmed = false), 3000);
  }

  // Editing ends when the island closes.
  $effect(() => {
    if (!ui.expanded) {
      editing = false;
      clearArmed = false;
    }
  });

  // The Clipboard tab goes away when the history is turned off.
  $effect(() => {
    if (tab === 'clipboard' && !preferences.value.clipboardHistory) tab = 'widgets';
  });
</script>

<div class="panel">
  <header>
    {#if editing}
      <span class="title">{t('island.editWidgets')}</span>
      <button class="done" onclick={() => (editing = false)}>{t('island.done')}</button>
    {:else}
      <div class="tabs">
        <button class:active={tab === 'widgets'} onclick={() => (tab = 'widgets')}>
          <SquaresFour size={14} weight="fill" /> {t('island.widgets')}
        </button>
        <button class:active={tab === 'tray'} onclick={() => (tab = 'tray')}>
          <Tray size={14} weight="fill" /> {t('island.tray')}
        </button>
        {#if preferences.value.clipboardHistory}
          <button class:active={tab === 'clipboard'} onclick={() => (tab = 'clipboard')}>
            <ClipboardText size={14} weight="fill" /> {t('island.clipboard')}
          </button>
        {/if}
      </div>
      <div class="tools">
        {#if tab === 'widgets'}
          <button class="tool" title={t('island.editWidgets')} onclick={() => (editing = true)}>
            <PencilSimple size={16} weight="fill" />
          </button>
        {:else if tab === 'clipboard' && clipboardHistory.entries.length}
          <button
            class="tool"
            class:armed={clearArmed}
            title={clearArmed ? t('island.clearClipboardConfirm') : t('island.clearClipboard')}
            onclick={clearHistory}
          >
            <Trash size={16} weight="fill" />
          </button>
        {/if}
        <button
          class="tool"
          title={t('island.settings')}
          onclick={() => invoke('settings_open').catch((err) => console.warn('settings', err))}
        >
          <GearSix size={17} weight="fill" />
        </button>
      </div>
    {/if}
  </header>
  {#if tab === 'widgets'}
    <div bind:clientWidth={widgetsWidth}><WidgetsView {editing} /></div>
  {:else if tab === 'clipboard'}
    <ClipboardView width={widgetsWidth} />
  {:else}
    <TrayView width={widgetsWidth} />
  {/if}
</div>

<style>
  .panel {
    padding: 6px 14px 16px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 28px;
    margin-bottom: 8px;
  }

  .tabs {
    display: flex;
    gap: 4px;
  }

  .tabs button {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 10px;
    border-radius: 12px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
    transition:
      background 150ms ease,
      color 150ms ease;
  }

  .tabs button:hover {
    color: var(--text);
  }

  .tabs button.active {
    background: var(--card);
    color: var(--text);
  }

  .title {
    padding-left: 4px;
    font-size: 13px;
    font-weight: 600;
  }

  .done {
    height: 24px;
    padding: 0 12px;
    border-radius: 12px;
    background: var(--accent);
    font-size: 13px;
    font-weight: 600;
  }

  .done:hover {
    filter: brightness(1.15);
  }

  .tools {
    display: flex;
    gap: 2px;
  }

  .tool {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 13px;
    color: var(--text-secondary);
  }

  .tool:hover {
    color: var(--text);
    background: var(--card);
  }

  .tool.armed,
  .tool.armed:hover {
    color: white;
    background: var(--danger);
  }
</style>
