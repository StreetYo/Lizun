<script lang="ts">
  import FileText from 'phosphor-svelte/lib/FileText';
  import Tray from 'phosphor-svelte/lib/Tray';
  import X from 'phosphor-svelte/lib/X';
  import { fileTray, type TrayFile } from '../lib/fileTray.svelte';
  import { t } from '../lib/i18n.svelte';

  let { width }: { width: number } = $props();
  let broken = $state<Record<string, boolean>>({});

  /** Finder-style middle ellipsis: "Emeral…om.png". */
  function shortName(name: string, max = 18): string {
    if (name.length <= max) return name;
    const keep = max - 1;
    return `${name.slice(0, Math.ceil(keep / 2))}…${name.slice(name.length - Math.floor(keep / 2))}`;
  }

  function showPreview(file: TrayFile): boolean {
    return Boolean(file.preview) && !broken[file.path];
  }

  // A press that moves a few pixels starts a system drag of the file.
  let pressed: { file: TrayFile; x: number; y: number } | null = null;

  function pointerMove(event: PointerEvent): void {
    if (!pressed) return;
    if (!(event.buttons & 1)) {
      pressed = null;
      return;
    }
    if (Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y) < 5) return;
    const { file } = pressed;
    pressed = null;
    void fileTray.dragOut(file).then(() => window.dispatchEvent(new Event('lizun:drag-out-finished')));
  }
</script>

<svelte:window onpointermove={pointerMove} onpointerup={() => (pressed = null)} />

<div class="tray" role="list" style:width={width ? `${width}px` : null}>
  {#each fileTray.files as file (file.path)}
    <div
      class="file"
      title={file.path}
      role="listitem"
      onpointerdown={(event) => {
        if (event.button === 0) pressed = { file, x: event.clientX, y: event.clientY };
      }}
      ondragstart={(event) => event.preventDefault()}
    >
      <div class="thumb">
        {#if showPreview(file)}
          <img src={file.preview} alt="" draggable="false" onerror={() => (broken[file.path] = true)} />
        {:else}
          <FileText size={34} weight="fill" />
          {#if file.extension}<span class="ext">{file.extension}</span>{/if}
        {/if}
      </div>
      <span class="name">{shortName(file.name)}</span>
      <button class="remove" title={t('island.removeFromTray')} onclick={() => fileTray.remove(file.path)}>
        <X size={9} weight="bold" />
      </button>
    </div>
  {:else}
    <div class="hint">
      <Tray size={22} weight="fill" />
      <span>{t('island.trayEmpty')}</span>
    </div>
  {/each}
</div>

<style>
  .tray {
    display: flex;
    gap: 6px;
    min-width: 560px;
    height: 164px;
    padding: 12px;
    box-sizing: border-box;
    border: 1.5px dashed rgba(255, 255, 255, 0.2);
    border-radius: 18px;
  }

  .file {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 82px;
    padding-top: 6px;
    border-radius: 10px;
  }

  .file:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .thumb {
    position: relative;
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    color: var(--text-secondary);
  }

  .thumb img {
    -webkit-user-drag: none;
    max-width: 56px;
    max-height: 56px;
    border-radius: 4px;
    box-shadow: 0 0 0 1.5px rgba(255, 255, 255, 0.85);
  }

  .ext {
    position: absolute;
    bottom: 6px;
    padding: 0 4px;
    border-radius: 3px;
    font-size: 8.5px;
    font-weight: 700;
    text-transform: uppercase;
    background: var(--accent);
    color: white;
  }

  .name {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 11.5px;
    line-height: 1.25;
    text-align: center;
    overflow-wrap: anywhere;
  }

  .remove {
    position: absolute;
    top: 2px;
    right: 8px;
    display: none;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 8px;
    background: var(--card-raised);
    color: var(--text);
  }

  .file:hover .remove {
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
