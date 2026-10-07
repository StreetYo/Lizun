<script lang="ts">
  import { onMount } from 'svelte';
  import Island from './components/Island.svelte';
  import { fileTray } from './lib/fileTray.svelte';
  import { isTauri } from './lib/platform';
  import { media } from './lib/media.svelte';
  import { widgetRegistry } from './lib/widgets/registry.svelte';

  onMount(() => {
    if (!isTauri) document.body.classList.add('preview');
    void media.start();
    void fileTray.restore();
    void widgetRegistry.load();
  });
</script>

<Island />
