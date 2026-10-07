<script lang="ts">
  import FileIcon from 'phosphor-svelte/lib/File';
  import Tray from 'phosphor-svelte/lib/Tray';
  import { fileTray } from '../lib/fileTray.svelte';
  import { media } from '../lib/media.svelte';

  /** Animations pause while the collapsed island is hidden behind the open panel. */
  let { active }: { active: boolean } = $props();
</script>

<div class="live">
  {#if media.nowPlaying}
    <div class="ear left">
      {#if media.artwork}
        <img class="art" src={media.artwork} alt="" />
      {:else}
        <div class="art"></div>
      {/if}
    </div>
    <div class="ear right">
      <div class="bars" class:playing={media.nowPlaying.playing && active} style:--tint={media.tint}>
        <span></span><span></span><span></span><span></span>
      </div>
    </div>
  {:else if fileTray.files.length}
    <div class="ear left"><Tray size={17} weight="fill" /></div>
    <div class="ear right count">{fileTray.files.length}<FileIcon size={13} weight="fill" /></div>
  {/if}
</div>

<style>
  .live {
    position: relative;
    height: 100%;
    color: var(--text);
  }

  .ear {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .left {
    left: 12px;
  }

  .right {
    right: 13px;
  }

  .art {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    object-fit: cover;
    background: var(--card-raised);
  }

  .count {
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .bars {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 16px;
  }

  .bars span {
    width: 3px;
    height: 100%;
    border-radius: 2px;
    background: var(--tint);
    transform: scaleY(0.25);
    transition: transform 300ms ease;
  }

  .bars.playing span {
    animation: bounce 0.9s ease-in-out infinite alternate;
  }

  .bars.playing span:nth-child(2) {
    animation-duration: 0.7s;
    animation-delay: -0.3s;
  }

  .bars.playing span:nth-child(3) {
    animation-duration: 1.1s;
    animation-delay: -0.6s;
  }

  .bars.playing span:nth-child(4) {
    animation-duration: 0.8s;
    animation-delay: -0.15s;
  }

  @keyframes bounce {
    from {
      transform: scaleY(0.25);
    }
    to {
      transform: scaleY(1);
    }
  }
</style>
