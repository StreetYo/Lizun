<script lang="ts">
  import Minus from 'phosphor-svelte/lib/Minus';
  import Plus from 'phosphor-svelte/lib/Plus';
  import SquaresFour from 'phosphor-svelte/lib/SquaresFour';
  import { t } from '../lib/i18n.svelte';
  import { preferences, WIDGET_ROWS } from '../lib/preferences.svelte';
  import { placeWidget, setWidgetEnabled, widgetLayout, type LayoutEntry } from '../lib/widgets/layout.svelte';
  import { widgetRegistry } from '../lib/widgets/registry.svelte';
  import CalendarWidget from './widgets/CalendarWidget.svelte';
  import MediaWidget from './widgets/MediaWidget.svelte';
  import MirrorWidget from './widgets/MirrorWidget.svelte';
  import WidgetFrame from './widgets/WidgetFrame.svelte';

  /** While editing, widgets can be dragged to another place or row, hidden and shown again. */
  let { editing = false }: { editing?: boolean } = $props();

  /** Space between two widgets and between two rows; the dividers sit in the middle. */
  const GAP = 29;
  const ROW_HEIGHT = 164;

  interface Place {
    row: number;
    index: number;
  }

  interface Drag {
    id: string;
    pointerId: number;
    /** The element that holds the pointer. */
    holder: Element;
    from: Place;
    /** Where the widget goes if it is dropped now. */
    target: Place;
    /** Where the pointer holds the widget, from its top left corner. */
    hold: { x: number; y: number };
    /** Top left corner of the widget in the view. */
    x: number;
    y: number;
  }

  const layout = $derived(widgetLayout(widgetRegistry.widgets, preferences.value.widgets));
  const shown = $derived(layout.filter((entry) => entry.enabled));
  const hidden = $derived(layout.filter((entry) => !entry.enabled));
  /** Ids of the shown widgets, row by row. */
  const saved = $derived(
    Array.from({ length: WIDGET_ROWS }, (_, row) => shown.filter((entry) => entry.row === row).map((entry) => entry.id)),
  );
  // The elements stay in one order and are placed with `translate`: an iframe that is moved in
  // the DOM reloads, so moving a widget, even into another row, must not move its element.
  const mounted = $derived([...shown].sort((a, b) => a.id.localeCompare(b.id)));
  const widths: Record<string, number> = $state({});

  let view = $state<HTMLElement>();
  let drag = $state<Drag | null>(null);

  /** The rows as they will be after the drop, so the other widgets step aside during a drag. */
  const rows = $derived.by(() => {
    if (!drag) return saved;
    const { id, target } = drag;
    const next = saved.map((ids) => ids.filter((other) => other !== id));
    next[target.row].splice(target.index, 0, id);
    return next;
  });
  /** While editing every row is on screen, so a widget can be dragged into an empty one. */
  const visibleRows = $derived(rows.flatMap((ids, row) => (editing || ids.length ? [row] : [])));
  const width = $derived(Math.max(0, ...visibleRows.map((row) => starts(rows[row]).end)));
  const height = $derived(Math.max(0, visibleRows.length * (ROW_HEIGHT + GAP) - GAP));

  const places = $derived.by(() => {
    const result = new Map<string, Place & { x: number; y: number }>();
    for (const row of visibleRows) {
      const y = top(row);
      starts(rows[row]).xs.forEach((x, index) => result.set(rows[row][index], { row, index, x, y }));
    }
    return result;
  });

  /** Left edges of the widgets of a row, and where the row ends. */
  function starts(ids: string[]): { xs: number[]; end: number } {
    const xs: number[] = [];
    let x = 0;
    for (const id of ids) {
      xs.push(x);
      x += (widths[id] ?? 0) + GAP;
    }
    return { xs, end: Math.max(0, x - GAP) };
  }

  function top(row: number): number {
    return Math.max(0, visibleRows.indexOf(row)) * (ROW_HEIGHT + GAP);
  }

  function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(Math.max(min, max), value));
  }

  function grab(event: PointerEvent, id: string): void {
    const place = places.get(id);
    // A drag whose element went away with the pointer never got its pointerup; it is over.
    const dragging = drag?.holder.hasPointerCapture(drag.pointerId);
    if (event.button !== 0 || dragging || !place || !view) return;
    event.preventDefault();
    const holder = event.currentTarget as Element;
    holder.setPointerCapture(event.pointerId);
    const origin = view.getBoundingClientRect();
    drag = {
      id,
      pointerId: event.pointerId,
      holder,
      from: { row: place.row, index: place.index },
      target: { row: place.row, index: place.index },
      hold: { x: event.clientX - origin.left - place.x, y: event.clientY - origin.top - place.y },
      x: place.x,
      y: place.y,
    };
  }

  /** Near the edge of a row that scrolls, a dragged widget scrolls it, so it can go anywhere. */
  const EDGE_SCROLL_ZONE = 48;
  const EDGE_SCROLL_STEP = 14;

  function scrollAtEdge(event: PointerEvent): void {
    const row = view?.parentElement;
    if (!row || row.scrollWidth <= row.clientWidth) return;
    const box = row.getBoundingClientRect();
    if (event.clientX < box.left + EDGE_SCROLL_ZONE) row.scrollLeft -= EDGE_SCROLL_STEP;
    else if (event.clientX > box.right - EDGE_SCROLL_ZONE) row.scrollLeft += EDGE_SCROLL_STEP;
  }

  function move(event: PointerEvent): void {
    if (!drag || event.pointerId !== drag.pointerId || !view) return;
    scrollAtEdge(event);
    // The view grows and moves while a widget goes into another row, so the pointer is read
    // against where the view is now.
    const origin = view.getBoundingClientRect();
    const size = widths[drag.id] ?? 0;
    const x = clamp(event.clientX - origin.left - drag.hold.x, 0, width - size);
    const y = clamp(event.clientY - origin.top - drag.hold.y, 0, height - ROW_HEIGHT);
    drag.x = x;
    drag.y = y;

    const row = visibleRows[clamp(Math.round(y / (ROW_HEIGHT + GAP)), 0, visibleRows.length - 1)];
    const ids = saved[row];
    const middles = starts(ids).xs.map((start, i) => start + (widths[ids[i]] ?? 0) / 2);
    let index: number;
    if (row === drag.from.row) {
      // Within its own row a widget steps aside once the leading edge of the dragged one passes
      // its middle.
      const from = drag.from.index;
      const passedAfter = middles.filter((middle, i) => i > from && middle < x + size).length;
      const passedBefore = middles.filter((middle, i) => i < from && middle > x).length;
      index = from + passedAfter - passedBefore;
    } else {
      // In another row it goes in front of the first widget whose middle is right of its own.
      index = middles.filter((middle) => middle < x + size / 2).length;
    }
    drag.target = { row, index };
  }

  /** Drops on pointerup; pointercancel or a lost pointer capture puts the widget back. */
  function drop(event: PointerEvent): void {
    if (!drag || event.pointerId !== drag.pointerId) return;
    const { id, from, target } = drag;
    if (event.type === 'pointerup' && (from.row !== target.row || from.index !== target.index)) {
      const others = saved[target.row].filter((other) => other !== id);
      // Saving shows the new order at once, so the widget slides from the pointer to its place.
      save(placeWidget(layout, id, target.row, others[target.index] ?? null));
    }
    drag = null;
  }

  // Closing the island ends editing and removes the element that holds the pointer, so no
  // pointerup or pointercancel comes to end a drag.
  $effect(() => {
    if (!editing) drag = null;
  });

  function setShown(entry: LayoutEntry, enabled: boolean): void {
    save(setWidgetEnabled(layout, entry.id, enabled));
  }

  function save(saving: Promise<void>): void {
    saving.catch((err) => console.warn('could not save the widget layout', err));
  }
</script>

{#if shown.length}
  <div class="view" class:editing bind:this={view} style:width="{width}px" style:height="{height}px">
    {#each visibleRows.slice(1) as row (row)}
      <div class="row-divider" style:top="{top(row) - (GAP + 1) / 2}px"></div>
    {/each}
    {#if editing}
      {#each visibleRows.filter((row) => !rows[row].length) as row (row)}
        <div class="empty-row" style:top="{top(row)}px" style:height="{ROW_HEIGHT}px">
          {t('island.dropInRow')}
        </div>
      {/each}
    {/if}
    {#each mounted as entry (entry.id)}
      {@const place = places.get(entry.id)}
      {@const held = drag?.id === entry.id ? drag : null}
      <div
        class="slot"
        class:first={place?.index === 0}
        class:dragged={held !== null}
        style:height="{ROW_HEIGHT}px"
        style:translate={held ? `${held.x}px ${held.y}px` : `${place?.x ?? 0}px ${place?.y ?? 0}px`}
        bind:clientWidth={widths[entry.id]}
      >
        {#if entry.id === 'builtin:media'}
          <MediaWidget />
        {:else if entry.id === 'builtin:mirror'}
          <MirrorWidget />
        {:else if entry.id === 'builtin:calendar'}
          <CalendarWidget />
        {:else if entry.manifest}
          <WidgetFrame widget={entry.manifest} />
        {/if}
        {#if editing}
          <!-- Covers the widget, so its buttons and frame do not take the pointer. -->
          <div
            class="grab"
            role="button"
            tabindex="-1"
            aria-label={t('island.moveWidget', { name: entry.name })}
            title={t('island.dragToMove')}
            onpointerdown={(event) => grab(event, entry.id)}
            onpointermove={move}
            onpointerup={drop}
            onpointercancel={drop}
            onlostpointercapture={drop}
          ></div>
          <button class="remove" title={t('island.hideWidget', { name: entry.name })} onclick={() => setShown(entry, false)}>
            <Minus size={11} weight="bold" />
          </button>
        {/if}
      </div>
    {/each}
  </div>
{:else}
  <div class="empty">
    <SquaresFour size={22} weight="fill" />
    <span>{editing ? t('island.allHiddenEditing') : t('island.allHidden')}</span>
  </div>
{/if}

{#if editing}
  <div class="hidden">
    {#each hidden as entry (entry.id)}
      <button class="add" title={entry.description} onclick={() => setShown(entry, true)}>
        <Plus size={11} weight="bold" />{entry.name}
      </button>
    {:else}
      <span class="note">{t('island.editNote')}</span>
    {/each}
  </div>
{/if}

<style>
  .view {
    position: relative;
  }

  .slot {
    position: absolute;
    top: 0;
    left: 0;
    display: flex;
    align-items: stretch;
    width: max-content;
    transition: translate 240ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  .slot:not(.first)::before {
    content: '';
    position: absolute;
    top: 6px;
    bottom: 6px;
    left: -15px;
    width: 1px;
    background: var(--divider);
    transition: opacity 160ms ease;
  }

  .row-divider {
    position: absolute;
    left: 6px;
    right: 6px;
    height: 1px;
    background: var(--divider);
    transition: opacity 160ms ease;
  }

  .editing .slot::before,
  .editing .row-divider {
    opacity: 0;
  }

  .slot.dragged {
    z-index: 2;
    transition: none;
  }

  .empty-row {
    position: absolute;
    left: 0;
    right: 0;
    display: grid;
    place-items: center;
    padding: 0 16px;
    border-radius: 18px;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14);
    color: var(--text-tertiary);
    font-size: 12.5px;
    font-weight: 500;
    text-align: center;
  }

  .grab {
    position: absolute;
    inset: -4px;
    z-index: 1;
    border-radius: 20px;
    background: rgba(255, 255, 255, 0.03);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14);
    cursor: grab;
    transition: background 150ms ease;
  }

  .grab:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .dragged .grab {
    cursor: grabbing;
    background: rgba(255, 255, 255, 0.07);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.24),
      0 10px 28px rgba(0, 0, 0, 0.6);
  }

  .remove {
    position: absolute;
    top: -6px;
    left: -6px;
    z-index: 3;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: #3a3940;
    color: var(--text);
    box-shadow: 0 0 0 2px var(--island-bg);
  }

  .remove:hover {
    background: var(--danger);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 560px;
    height: 164px;
    border-radius: 18px;
    background: var(--card);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 500;
  }

  /* Takes the width of the widgets instead of widening the island, and stays in view while the
     row scrolls. */
  .hidden {
    position: sticky;
    left: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    width: 0;
    min-width: 100%;
    min-height: 26px;
    margin-top: 12px;
  }

  .add {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 11px 0 9px;
    border-radius: 13px;
    background: var(--card);
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .add:hover {
    background: var(--card-raised);
    color: var(--text);
  }

  .note {
    font-size: 12.5px;
    color: var(--text-tertiary);
  }
</style>
