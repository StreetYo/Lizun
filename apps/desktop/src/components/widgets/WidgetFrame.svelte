<script lang="ts">
  import { untrack } from 'svelte';
  import { WIDGET_ORIGIN } from '../../lib/platform';
  import { attachWidget } from '../../lib/widgets/host';
  import type { WidgetManifest } from '../../lib/widgets/registry.svelte';
  import { widgetSettings } from '../../lib/widgets/settings.svelte';

  let { widget }: { widget: WidgetManifest } = $props();

  /** A page that navigates its frame again right after every start is given up on after this
   * many quick restarts, so it cannot keep the island busy. */
  const MAX_QUICK_RESTARTS = 3;
  const QUICK_RESTART_MS = 5_000;

  // Bumped to start the widget over in a new frame after its page navigated the old one.
  let restarts = $state(0);
  let gaveUp = $state(false);
  let quickRestarts = 0;
  let lastRestart = 0;

  function restart(): void {
    const now = Date.now();
    quickRestarts = now - lastRestart < QUICK_RESTART_MS ? quickRestarts + 1 : 0;
    lastRestart = now;
    if (quickRestarts >= MAX_QUICK_RESTARTS) {
      console.warn(`[widget ${widget.id}] keeps navigating its frame, so it was stopped`);
      gaveUp = true;
    } else {
      restarts++;
    }
  }

  // The addresses in the widget's `url` settings are part of its page's CSP, which the frame
  // keeps until its page loads again: when one changes, the widget starts over with the new one.
  const addresses = $derived(
    widget.settings
      .filter((setting) => setting.type === 'url')
      .map((setting) => String(widgetSettings.values[widget.id]?.[setting.key] ?? setting.default))
      .join(' '),
  );

  // A plain copy: reactive proxies cannot be sent through postMessage.
  const attach = (iframe: HTMLIFrameElement) => untrack(() => attachWidget(iframe, $state.snapshot(widget), restart));
</script>

<!--
  `sandbox="allow-scripts"` without `allow-same-origin` gives the widget an opaque origin:
  no access to the island page, its storage, or the Tauri IPC. A new frame is created for each
  restart, because the bridge tells frames apart by their window, which survives navigation.
-->
{#key `${restarts} ${addresses}`}
  {#if gaveUp}
    <div style:width="{widget.width}px"></div>
  {:else}
    <iframe
      {@attach attach}
      title={widget.name}
      src="{WIDGET_ORIGIN}/{widget.id}/"
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      style:width="{widget.width}px"
    ></iframe>
  {/if}
{/key}

<style>
  iframe {
    flex: none;
    height: 100%;
    border: none;
    background: transparent;
    color-scheme: dark;
  }
</style>
