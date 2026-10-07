<script lang="ts">
  import ShareNetwork from 'phosphor-svelte/lib/ShareNetwork';
  import Tray from 'phosphor-svelte/lib/Tray';
  import { fileTray } from '../lib/fileTray.svelte';

  let { active }: { active: string | null } = $props();
</script>

<div class="zones">
  <div class="zone tray" class:active={active === 'tray'} data-drop-zone="tray">
    <Tray size={24} weight="fill" />
    <span>Files Tray</span>
    {#if fileTray.files.length}<small>{fileTray.files.length} files</small>{/if}
  </div>
  <div class="zone share" class:active={active === 'share'} data-drop-zone="share">
    <ShareNetwork size={24} weight="fill" />
    <span>Share</span>
    <small>coming soon</small>
  </div>
</div>

<style>
  .zones {
    display: flex;
    gap: 12px;
    width: 520px;
    height: 150px;
    padding: 14px;
    box-sizing: border-box;
  }

  .zone {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
    padding: 0 20px;
    border-radius: 22px;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-secondary);
    transition:
      background 150ms ease,
      border-color 150ms ease,
      color 150ms ease;
  }

  .zone small {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-tertiary);
  }

  .tray {
    border: 1.5px dashed rgba(255, 255, 255, 0.24);
  }

  .tray.active {
    border-color: rgba(255, 255, 255, 0.6);
    background: rgba(255, 255, 255, 0.05);
    color: var(--text);
  }

  .share {
    background: var(--accent-deep);
    color: var(--accent);
  }

  .share.active {
    background: #0b2c4f;
  }
</style>
