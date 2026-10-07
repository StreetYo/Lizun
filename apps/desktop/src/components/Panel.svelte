<script lang="ts">
  import GearSix from 'phosphor-svelte/lib/GearSix';
  import MagicWand from 'phosphor-svelte/lib/MagicWand';
  import Tray from 'phosphor-svelte/lib/Tray';
  import NookView from './NookView.svelte';
  import TrayView from './TrayView.svelte';

  let { tab = $bindable('nook') }: { tab: 'nook' | 'tray' } = $props();

  // The tray takes the width of the widget row, so switching tabs does not shrink the island
  // out from under the cursor.
  let nookWidth = $state(0);
</script>

<div class="panel">
  <header>
    <div class="tabs">
      <button class:active={tab === 'nook'} onclick={() => (tab = 'nook')}>
        <MagicWand size={14} weight="fill" /> Nook
      </button>
      <button class:active={tab === 'tray'} onclick={() => (tab = 'tray')}>
        <Tray size={14} weight="fill" /> Tray
      </button>
    </div>
    <button class="settings" title="Settings">
      <GearSix size={17} weight="fill" />
    </button>
  </header>
  {#if tab === 'nook'}
    <div bind:clientWidth={nookWidth}><NookView /></div>
  {:else}
    <TrayView width={nookWidth} />
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

  .settings {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 13px;
    color: var(--text-secondary);
  }

  .settings:hover {
    color: var(--text);
    background: var(--card);
  }
</style>
