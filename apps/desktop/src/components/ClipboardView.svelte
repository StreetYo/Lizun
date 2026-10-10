<script lang="ts">
  import Check from 'phosphor-svelte/lib/Check';
  import ClipboardText from 'phosphor-svelte/lib/ClipboardText';
  import FileText from 'phosphor-svelte/lib/FileText';
  import Folder from 'phosphor-svelte/lib/Folder';
  import ImageIcon from 'phosphor-svelte/lib/Image';
  import X from 'phosphor-svelte/lib/X';
  import { clipboardHistory, type ClipboardEntry, type ClipboardFile } from '../lib/clipboard.svelte';
  import { describeFile } from '../lib/fileTray.svelte';
  import { t } from '../lib/i18n.svelte';
  import { fileUrl } from '../lib/platform';

  let { width }: { width: number } = $props();

  let now = $state(Date.now());
  let copied = $state<number | null>(null);
  let failed = $state<number | null>(null);
  let broken = $state<Record<number, boolean>>({});
  let brokenFiles = $state<Record<string, boolean>>({});
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  // "5 min ago" keeps up while the tab is open.
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => {
      clearInterval(timer);
      clearTimeout(copiedTimer);
    };
  });

  function ago(time: number): string {
    const minutes = Math.floor((now - time) / 60_000);
    if (minutes < 1) return t('clipboard.justNow');
    if (minutes < 60) return t('clipboard.minutesAgo', { count: minutes });
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return t('clipboard.hoursAgo', { count: hours });
    return t('clipboard.daysAgo', { count: Math.floor(hours / 24) });
  }

  async function copy(entry: ClipboardEntry): Promise<void> {
    clearTimeout(copiedTimer);
    try {
      await clipboardHistory.copy(entry.id);
      copied = entry.id;
      failed = null;
    } catch (err) {
      console.warn('clipboard copy', err);
      copied = null;
      failed = entry.id;
    }
    copiedTimer = setTimeout(() => (copied = failed = null), 1400);
  }

  /** Up to four files fill the 2×2 grid; with more, the last cell counts the files not shown,
   * as phones do. */
  function fileTiles(entry: { files: ClipboardFile[]; count: number }): { files: ClipboardFile[]; more: number } {
    if (entry.count <= 4) return { files: entry.files, more: 0 };
    return { files: entry.files.slice(0, 3), more: entry.count - 3 };
  }

  function title(entry: ClipboardEntry): string {
    if (entry.kind === 'files' && entry.files.every((file) => file.missing)) return t('clipboard.filesGone');
    return t('clipboard.copy');
  }

  /** The wheel scrolls the row sideways. */
  function wheel(event: WheelEvent): void {
    const list = event.currentTarget as HTMLElement;
    if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    event.preventDefault();
    list.scrollLeft += event.deltaY;
  }
</script>

{#snippet fileTile(file: ClipboardFile, big: boolean)}
  {@const info = describeFile(file.path)}
  <span class="tile" class:missing={file.missing} title={file.path}>
    {#if info.preview && !file.missing && !brokenFiles[file.path]}
      <img src={info.preview} alt="" draggable="false" onerror={() => (brokenFiles[file.path] = true)} />
    {:else if file.folder}
      <Folder size={big ? 54 : 26} weight="fill" />
    {:else}
      <FileText size={big ? 54 : 26} weight="fill" />
      {#if info.extension}<span class="ext" class:big>{info.extension}</span>{/if}
    {/if}
  </span>
{/snippet}

<div
  class="history"
  class:empty={!clipboardHistory.entries.length}
  role="list"
  style:width={width ? `${width}px` : null}
  onwheel={wheel}
>
  {#each clipboardHistory.entries as entry (entry.id)}
    <div class="entry" role="listitem">
      <button
        class="copy"
        class:picture={entry.kind === 'picture'}
        title={title(entry)}
        onclick={() => copy(entry)}
      >
        {#if entry.kind === 'text'}
          <span class="text">{entry.text}{entry.cut ? '…' : ''}</span>
        {:else if entry.kind === 'files'}
          {@const tiles = fileTiles(entry)}
          <span class="files" class:single={entry.count === 1}>
            <!-- By place: the app that copied the files may list one twice. -->
            {#each tiles.files as file, index (index)}
              {@render fileTile(file, entry.count === 1)}
            {/each}
            {#if tiles.more}<span class="tile more">+{tiles.more}</span>{/if}
          </span>
        {:else if entry.path && !broken[entry.id]}
          <img src={fileUrl(entry.path)} alt="" draggable="false" onerror={() => (broken[entry.id] = true)} />
        {:else}
          <span class="placeholder"><ImageIcon size={30} weight="fill" /></span>
        {/if}
        <span class="meta">
          <span class="time">{ago(entry.copiedAt)}</span>
          {#if entry.kind === 'picture'}
            <span class="size">{entry.width}×{entry.height}</span>
          {:else if entry.kind === 'files'}
            <span class="size">
              {entry.count === 1 ? describeFile(entry.files[0].path).name : t('clipboard.files', { count: entry.count })}
            </span>
          {/if}
        </span>
        {#if copied === entry.id || failed === entry.id}
          <span class="flash" class:bad={failed === entry.id}>
            {#if copied === entry.id}<Check size={18} weight="bold" /> {t('clipboard.copied')}{:else}{t('clipboard.failed')}{/if}
          </span>
        {/if}
      </button>
      <button class="remove" title={t('clipboard.remove')} onclick={() => clipboardHistory.remove(entry.id)}>
        <X size={9} weight="bold" />
      </button>
    </div>
  {:else}
    <div class="hint">
      <ClipboardText size={22} weight="fill" />
      <span>{t('clipboard.empty')}</span>
    </div>
  {/each}
</div>

<style>
  .history {
    display: flex;
    gap: 8px;
    min-width: 560px;
    height: 164px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    box-sizing: border-box;
  }

  .history.empty {
    border: 1.5px dashed rgba(255, 255, 255, 0.2);
    border-radius: 18px;
  }

  .entry {
    position: relative;
    flex: none;
    width: 148px;
    height: 100%;
  }

  .copy {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    height: 100%;
    padding: 10px 10px 8px;
    box-sizing: border-box;
    overflow: hidden;
    border-radius: 14px;
    background: var(--card);
    text-align: left;
    transition: background 150ms ease;
  }

  .copy:hover {
    background: var(--card-raised);
  }

  .text {
    flex: 1;
    display: -webkit-box;
    -webkit-line-clamp: 7;
    line-clamp: 7;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 12px;
    line-height: 1.35;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .copy.picture img,
  .placeholder {
    flex: 1;
    min-height: 0;
    width: 100%;
    object-fit: contain;
    -webkit-user-drag: none;
  }

  .copy.picture img {
    border-radius: 6px;
  }

  .placeholder {
    display: grid;
    place-items: center;
    color: var(--text-secondary);
  }

  .meta {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    font-size: 10.5px;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .time {
    flex: none;
  }

  .size {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-tertiary);
  }

  .files {
    flex: 1;
    min-height: 0;
    align-self: center;
    aspect-ratio: 1;
    display: grid;
    grid-template: 1fr 1fr / 1fr 1fr;
    gap: 4px;
  }

  .files.single {
    grid-template: 1fr / 1fr;
  }

  .tile {
    position: relative;
    display: grid;
    place-items: center;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-secondary);
  }

  .tile img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    -webkit-user-drag: none;
  }

  .tile.missing {
    opacity: 0.35;
  }

  .tile.more {
    font-size: 15px;
    font-weight: 700;
    color: var(--text);
  }

  .ext {
    position: absolute;
    bottom: 7px;
    max-width: calc(100% - 8px);
    overflow: hidden;
    padding: 0 3px;
    border-radius: 3px;
    font-size: 7.5px;
    font-weight: 700;
    text-transform: uppercase;
    background: var(--accent);
    color: white;
  }

  .ext.big {
    bottom: 26px;
    padding: 0 4px;
    font-size: 10px;
  }

  .flash {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border-radius: inherit;
    background: rgba(10, 132, 255, 0.92);
    font-size: 13px;
    font-weight: 600;
    color: white;
    animation: flash 180ms ease;
  }

  .flash.bad {
    background: rgba(255, 69, 58, 0.92);
  }

  @keyframes flash {
    from {
      opacity: 0;
    }
  }

  .remove {
    position: absolute;
    top: 6px;
    right: 6px;
    display: none;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 9px;
    background: var(--card-raised);
    box-shadow: 0 0 0 1px var(--divider);
    color: var(--text);
  }

  .entry:hover .remove {
    display: grid;
  }

  .hint {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-secondary);
  }
</style>
