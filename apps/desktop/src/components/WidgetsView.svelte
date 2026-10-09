<script lang="ts">
  import Minus from 'phosphor-svelte/lib/Minus';
  import Plus from 'phosphor-svelte/lib/Plus';
  import SquaresFour from 'phosphor-svelte/lib/SquaresFour';
  import { t } from '../lib/i18n.svelte';
  import { preferences } from '../lib/preferences.svelte';
  import { Reorder } from '../lib/reorder.svelte';
  import { moveWidget, setWidgetEnabled, widgetLayout, type LayoutEntry } from '../lib/widgets/layout.svelte';
  import { widgetRegistry } from '../lib/widgets/registry.svelte';
  import CalendarWidget from './widgets/CalendarWidget.svelte';
  import MediaWidget from './widgets/MediaWidget.svelte';
  import MirrorWidget from './widgets/MirrorWidget.svelte';
  import WidgetFrame from './widgets/WidgetFrame.svelte';

  /** While editing, widgets can be dragged to another place, hidden and shown again. */
  let { editing = false }: { editing?: boolean } = $props();

  /** Space between two widgets; the divider sits in the middle. */
  const GAP = 29;

  const layout = $derived(widgetLayout(widgetRegistry.widgets, preferences.value.widgets));
  const shown = $derived(layout.filter((entry) => entry.enabled));
  const hidden = $derived(layout.filter((entry) => !entry.enabled));
  const positions = $derived(new Map(shown.map((entry, index) => [entry.id, index])));
  // The elements stay in one order and CSS `order` places them: an iframe that is moved in the
  // DOM reloads, so moving a widget must not move its element.
  const mounted = $derived([...shown].sort((a, b) => a.id.localeCompare(b.id)));
  const slots: Record<string, HTMLElement> = {};

  const reorder = new Reorder('x', GAP, (from, to) => {
    // Hidden widgets keep their place in the full list; the dragged one goes next to the
    // widget it was dropped on.
    const index = (i: number) => layout.findIndex((entry) => entry.id === shown[i].id);
    save(moveWidget(layout, index(from), index(to)));
  });

  function setShown(entry: LayoutEntry, enabled: boolean): void {
    save(setWidgetEnabled(layout, entry.id, enabled));
  }

  function save(saving: Promise<void>): void {
    saving.catch((err) => console.warn('could not save the widget layout', err));
  }
</script>

<div class="row" class:editing class:instant={reorder.instant}>
  {#each mounted as entry (entry.id)}
    {@const index = positions.get(entry.id) ?? 0}
    <div
      class="slot"
      class:first={index === 0}
      class:dragged={reorder.dragging === index}
      style:order={index}
      style:translate="{reorder.offset(index)}px 0"
      bind:this={slots[entry.id]}
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
          onpointerdown={(event) => reorder.start(event, index, shown.map((item) => slots[item.id]))}
          onpointermove={(event) => reorder.move(event)}
          onpointerup={(event) => reorder.end(event)}
          onpointercancel={(event) => reorder.end(event)}
        ></div>
        <button class="remove" title={t('island.hideWidget', { name: entry.name })} onclick={() => setShown(entry, false)}>
          <Minus size={11} weight="bold" />
        </button>
      {/if}
    </div>
  {:else}
    <div class="empty">
      <SquaresFour size={22} weight="fill" />
      <span>{editing ? t('island.allHiddenEditing') : t('island.allHidden')}</span>
    </div>
  {/each}
</div>

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
  .row {
    display: flex;
    align-items: stretch;
    gap: 29px;
    height: 164px;
  }

  .slot {
    position: relative;
    display: flex;
    flex: none;
    align-items: stretch;
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

  .editing .slot::before {
    opacity: 0;
  }

  .slot.dragged,
  .instant .slot {
    transition: none;
  }

  .slot.dragged {
    z-index: 2;
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
    border-radius: 18px;
    background: var(--card);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 500;
  }

  /* Takes the width of the row instead of widening the island. */
  .hidden {
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
