<script lang="ts">
  import MusicNotes from 'phosphor-svelte/lib/MusicNotes';
  import Pause from 'phosphor-svelte/lib/Pause';
  import Play from 'phosphor-svelte/lib/Play';
  import SkipBack from 'phosphor-svelte/lib/SkipBack';
  import SkipForward from 'phosphor-svelte/lib/SkipForward';
  import { media, type MediaCommand } from '../../lib/media.svelte';
  import { ui } from '../../lib/ui.svelte';

  let now = $state(Date.now());

  // Advance the progress bar only while it can be seen.
  $effect(() => {
    if (!ui.expanded || !media.nowPlaying?.playing) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(timer);
  });

  const position = $derived(media.positionAt(now));
  const duration = $derived(media.nowPlaying?.durationMs ?? null);
  const progress = $derived(position !== null && duration ? position / duration : 0);

  function formatTime(ms: number): string {
    const total = Math.floor(ms / 1000);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const seconds = String(total % 60).padStart(2, '0');
    return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${seconds}` : `${minutes}:${seconds}`;
  }

  function control(command: MediaCommand): void {
    media.control(command).catch((err) => console.warn('media control', err));
  }
</script>

<section class="media">
  {#if media.nowPlaying}
    {@const np = media.nowPlaying}
    <div class="top">
      {#if media.artwork}
        <img class="art" src={media.artwork} alt="" />
      {:else}
        <div class="art placeholder"><MusicNotes size={30} weight="fill" /></div>
      {/if}
      <div class="info">
        <div class="title">{np.title || 'Unknown title'}</div>
        <div class="artist">{np.artist || np.sourceApp}</div>
        <div class="controls">
          <button disabled={!np.canPrevious} onclick={() => control('previous')} title="Previous">
            <SkipBack size={17} weight="fill" />
          </button>
          <button class="play" disabled={!np.canToggle} onclick={() => control('togglePlayPause')} title="Play/Pause">
            {#if np.playing}<Pause size={21} weight="fill" />{:else}<Play size={21} weight="fill" />{/if}
          </button>
          <button disabled={!np.canNext} onclick={() => control('next')} title="Next">
            <SkipForward size={17} weight="fill" />
          </button>
        </div>
      </div>
    </div>
    {#if duration}
      <div class="progress">
        <div class="track"><div class="fill" style:width="{progress * 100}%"></div></div>
        <div class="times">
          <span>{formatTime(position ?? 0)}</span>
          <span>{formatTime(duration)}</span>
        </div>
      </div>
    {/if}
  {:else}
    <div class="empty">
      <MusicNotes size={28} weight="fill" />
      <span>Nothing playing</span>
    </div>
  {/if}
</section>

<style>
  .media {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    width: 300px;
    padding: 4px 2px 0;
    box-sizing: border-box;
  }

  .top {
    display: flex;
    gap: 14px;
  }

  .art {
    flex: none;
    width: 96px;
    height: 96px;
    border-radius: 16px;
    object-fit: cover;
    background: var(--card);
  }

  .placeholder {
    display: grid;
    place-items: center;
    color: var(--text-tertiary);
  }

  .info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding-top: 6px;
  }

  .title {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 15px;
    font-weight: 650;
    line-height: 1.2;
  }

  .artist {
    margin-top: 3px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12.5px;
    color: var(--text-secondary);
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: auto;
  }

  .controls button {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 13px;
    color: var(--text);
    transition:
      background 120ms ease,
      transform 120ms ease;
  }

  .controls button:hover:not(:disabled) {
    background: var(--card-raised);
  }

  .controls button:active:not(:disabled) {
    transform: scale(0.9);
  }

  .controls button:disabled {
    color: var(--text-tertiary);
  }

  .progress {
    margin-top: 10px;
  }

  .track {
    height: 5px;
    border-radius: 3px;
    background: #2a292e;
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: 3px;
    background: #eaeaea;
  }

  .times {
    display: flex;
    justify-content: space-between;
    margin-top: 4px;
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
  }

  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-secondary);
  }
</style>
