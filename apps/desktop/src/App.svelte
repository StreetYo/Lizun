<script lang="ts">
  import { onMount } from 'svelte';
  import Island from './components/Island.svelte';
  import { fileTray } from './lib/fileTray.svelte';
  import { isTauri, listen } from './lib/platform';
  import { media } from './lib/media.svelte';
  import { preferences } from './lib/preferences.svelte';
  import { widgetRegistry } from './lib/widgets/registry.svelte';

  onMount(() => {
    if (!isTauri) document.body.classList.add('preview');
    void preferences.start();
    void media.start();
    void fileTray.restore();
    void widgetRegistry.load();
    // The settings window reloads widgets after the user adds or removes one.
    const stop = listen('widgets://changed', () => void widgetRegistry.load());
    return () => void stop.then((unlisten) => unlisten());
  });
</script>

<Island />
