<script lang="ts">
  import { widgetRegistry } from '../lib/widgets/registry.svelte';
  import CalendarWidget from './widgets/CalendarWidget.svelte';
  import MediaWidget from './widgets/MediaWidget.svelte';
  import MirrorWidget from './widgets/MirrorWidget.svelte';
  import WidgetFrame from './widgets/WidgetFrame.svelte';
</script>

<div class="row">
  <MediaWidget />
  <div class="divider"></div>
  <MirrorWidget />
  <div class="divider"></div>
  <CalendarWidget />
  {#each widgetRegistry.widgets as widget (widget.id)}
    <div class="divider"></div>
    <WidgetFrame {widget} />
  {/each}
</div>

<style>
  .row {
    display: flex;
    align-items: stretch;
    height: 164px;
  }

  .divider {
    flex: none;
    width: 1px;
    margin: 6px 14px;
    background: var(--divider);
  }
</style>
