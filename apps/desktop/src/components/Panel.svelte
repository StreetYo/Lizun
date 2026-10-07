<script lang="ts">
  import GearSix from 'phosphor-svelte/lib/GearSix';
  import PencilSimple from 'phosphor-svelte/lib/PencilSimple';
  import SquaresFour from 'phosphor-svelte/lib/SquaresFour';
  import Tray from 'phosphor-svelte/lib/Tray';
  import { invoke } from '../lib/platform';
  import { ui } from '../lib/ui.svelte';
  import TrayView from './TrayView.svelte';
  import WidgetsView from './WidgetsView.svelte';

  let { tab = $bindable('widgets') }: { tab: 'widgets' | 'tray' } = $props();

  // The tray takes the width of the widget row, so switching tabs does not shrink the island
  // out from under the cursor.
  let widgetsWidth = $state(0);
  let editing = $state(false);

  // Editing ends when the island closes.
  $effect(() => {
    if (!ui.expanded) editing = false;
  });
</script>

<div class="panel">
  <header>
    {#if editing}
      <span class="title">Edit widgets</span>
      <button class="done" onclick={() => (editing = false)}>Done</button>
    {:else}
      <div class="tabs">
        <button class:active={tab === 'widgets'} onclick={() => (tab = 'widgets')}>
          <SquaresFour size={14} weight="fill" /> Widgets
        </button>
        <button class:active={tab === 'tray'} onclick={() => (tab = 'tray')}>
          <Tray size={14} weight="fill" /> Tray
        </button>
      </div>
      <div class="tools">
        {#if tab === 'widgets'}
          <button class="tool" title="Edit widgets" onclick={() => (editing = true)}>
            <PencilSimple size={16} weight="fill" />
          </button>
        {/if}
        <button
          class="tool"
          title="Settings"
          onclick={() => invoke('settings_open').catch((err) => console.warn('settings', err))}
        >
          <GearSix size={17} weight="fill" />
        </button>
      </div>
    {/if}
  </header>
  {#if tab === 'widgets'}
    <div bind:clientWidth={widgetsWidth}><WidgetsView {editing} /></div>
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
</style>
