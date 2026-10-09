<script lang="ts">
  import { onMount } from 'svelte';
  import Island from './components/Island.svelte';
  import { calendar } from './lib/calendar.svelte';
  import { fileTray } from './lib/fileTray.svelte';
  import { keyboard } from './lib/keyboard.svelte';
  import { isTauri, listen } from './lib/platform';
  import { media } from './lib/media.svelte';
  import { preferences } from './lib/preferences.svelte';
  import { widgetRegistry } from './lib/widgets/registry.svelte';
  import { widgetSettings } from './lib/widgets/settings.svelte';

  onMount(() => {
    if (!isTauri) document.body.classList.add('preview');
    void preferences.start();
    void media.start();
    void calendar.start();
    void fileTray.restore();
    void widgetRegistry.load();
    void widgetSettings.start();
    void keyboard.start();
    // The settings window reloads widgets after the user adds or removes one.
    const stop = listen('widgets://changed', () => {
      void widgetRegistry.load();
      void widgetSettings.load();
    });
    return () => void stop.then((unlisten) => unlisten());
  });
</script>

<Island />
