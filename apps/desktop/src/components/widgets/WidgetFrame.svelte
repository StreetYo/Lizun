<script lang="ts">
  import { onMount } from 'svelte';
  import { WIDGET_ORIGIN } from '../../lib/platform';
  import { attachWidget } from '../../lib/widgets/host';
  import type { WidgetManifest } from '../../lib/widgets/registry.svelte';

  let { widget }: { widget: WidgetManifest } = $props();
  let iframe: HTMLIFrameElement;

  // A plain copy: reactive proxies cannot be sent through postMessage.
  onMount(() => attachWidget(iframe, $state.snapshot(widget)));
</script>

<!--
  `sandbox="allow-scripts"` without `allow-same-origin` gives the widget an opaque origin:
  no access to the island page, its storage, or the Tauri IPC.
-->
<iframe
  bind:this={iframe}
  title={widget.name}
  src="{WIDGET_ORIGIN}/{widget.id}/"
  sandbox="allow-scripts"
  referrerpolicy="no-referrer"
  style:width="{widget.width}px"
></iframe>

<style>
  iframe {
    flex: none;
    height: 100%;
    border: none;
    background: transparent;
    color-scheme: dark;
  }
</style>
