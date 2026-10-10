<script lang="ts">
  import CaretLeft from 'phosphor-svelte/lib/CaretLeft';
  import CaretRight from 'phosphor-svelte/lib/CaretRight';
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

  /** The island stays this far from the sides of the screen; widgets that do not fit scroll. */
  const SCREEN_MARGIN = 48;
  /** The panel's own padding left and right, see `.panel`. */
  const PANEL_PADDING = 28;
  /** Room around the widgets inside the scrolling row for the edit mode's buttons, which stick
   * out of the widgets; the row would cut them off. */
  const ROW_BLEED = 8;

  let screenWidth = $state(window.innerWidth);
  const maxWidth = $derived(Math.max(320, screenWidth - 2 * SCREEN_MARGIN - PANEL_PADDING));
  let scroller = $state<HTMLElement>();
  let moreLeft = $state(false);
  let moreRight = $state(false);

  /** Which sides have widgets out of sight, and the width the other tabs take. */
  function measure(): void {
    if (!scroller) return;
    const { scrollLeft, clientWidth, scrollWidth } = scroller;
    moreLeft = scrollLeft > 1;
    moreRight = scrollLeft + clientWidth < scrollWidth - 1;
    widgetsWidth = clientWidth - 2 * ROW_BLEED;
  }

  $effect(() => {
    const row = scroller;
    if (!row) return;
    const observer = new ResizeObserver(measure);
    observer.observe(row);
    for (const child of row.children) observer.observe(child);
    measure();
    return () => observer.disconnect();
  });

  /** The wheel scrolls the row sideways. Over a widget the wheel goes to the widget's own page,
   * so this only works between and around widgets; the arrows work everywhere. */
  function wheel(event: WheelEvent): void {
    if (!scroller || !(moreLeft || moreRight) || Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
    scroller.scrollLeft += event.deltaY;
  }

  /** Scrolls by most of the visible width, keeping a widget's worth in view for orientation. */
  function page(direction: -1 | 1): void {
    if (!scroller) return;
    scroller.scrollBy({ left: direction * Math.max(160, scroller.clientWidth - 160), behavior: 'smooth' });
  }

  /** Clearing the clipboard history takes a second click: the first turns the trash button into
   * a red "Clear all?" button, which goes back after a few seconds or when the cursor leaves. */
  let clearArmed = $state(false);
  let clearTimer: ReturnType<typeof setTimeout> | undefined;

  function disarmClear(): void {
    clearTimeout(clearTimer);
    clearArmed = false;
  }

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
      disarmClear();
    }
  });

  $effect(() => {
    if (tab !== 'clipboard') disarmClear();
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
          {#if clearArmed}
            <button class="confirm-clear" onclick={clearHistory} onpointerleave={disarmClear}>
              <Trash size={14} weight="fill" /> {t('island.clearClipboardAsk')}
            </button>
          {:else}
            <button class="tool" title={t('island.clearClipboard')} onclick={clearHistory}>
              <Trash size={16} weight="fill" />
            </button>
          {/if}
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
    <div class="widgets" class:more-left={moreLeft} class:more-right={moreRight}>
      <div
        class="row"
        bind:this={scroller}
        style:max-width="{maxWidth + 2 * ROW_BLEED}px"
        style:--bleed="{ROW_BLEED}px"
        onscroll={measure}
        onwheel={wheel}
      >
        <WidgetsView {editing} />
      </div>
      {#if moreLeft}
        <button class="page left" title={t('island.scrollLeft')} onclick={() => page(-1)}>
          <CaretLeft size={14} weight="bold" />
        </button>
      {/if}
      {#if moreRight}
        <button class="page right" title={t('island.scrollRight')} onclick={() => page(1)}>
          <CaretRight size={14} weight="bold" />
        </button>
      {/if}
    </div>
  {:else if tab === 'clipboard'}
    <ClipboardView width={widgetsWidth} />
  {:else}
    <TrayView width={widgetsWidth} />
  {/if}
</div>

<svelte:window bind:innerWidth={screenWidth} />

<style>
  .panel {
    padding: 6px 14px 16px;
  }

  .widgets {
    position: relative;
    --fade-left: 0px;
    --fade-right: 0px;
  }

  .widgets.more-left {
    --fade-left: 48px;
  }

  .widgets.more-right {
    --fade-right: 48px;
  }

  .row {
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    padding: var(--bleed);
    margin: calc(-1 * var(--bleed));
    /* Widgets out of sight fade out at the edge they are behind. */
    mask-image: linear-gradient(
      to right,
      transparent 0,
      #000 var(--fade-left),
      #000 calc(100% - var(--fade-right)),
      transparent 100%
    );
  }

  .page {
    position: absolute;
    top: 50%;
    z-index: 4;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    margin-top: -13px;
    border-radius: 13px;
    background: var(--card-raised);
    color: var(--text);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.6);
    opacity: 0.85;
    transition:
      opacity 150ms ease,
      background 150ms ease;
  }

  .page:hover {
    opacity: 1;
    background: #3a3940;
  }

  .page.left {
    left: -8px;
  }

  .page.right {
    right: -8px;
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

  .confirm-clear {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 11px 0 9px;
    border-radius: 13px;
    background: var(--danger);
    color: white;
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    animation: confirm-in 160ms ease;
  }

  .confirm-clear:hover {
    filter: brightness(1.1);
  }

  @keyframes confirm-in {
    from {
      opacity: 0;
      scale: 0.9;
    }
  }
</style>
