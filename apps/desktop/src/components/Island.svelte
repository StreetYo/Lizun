<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { debug, frameStats } from '../lib/debug';
  import { fileTray } from '../lib/fileTray.svelte';
  import { keyboard } from '../lib/keyboard.svelte';
  import {
    CLOSE_DELAY_MS,
    COLLAPSED_HEIGHT,
    COLLAPSED_RADIUS,
    CAPSULE_MIN_WIDTH,
    CONCEALED_WIDTH_SCALE,
    DROP_SIZE,
    EXPANDED_RADIUS,
    HOT_ZONE_HEIGHT,
    HOVER_MARGIN,
    PILL_WIDTH,
    SHADOW_MARGIN,
    SHORTCUT_CLOSE_DELAY_MS,
    type IslandSize,
  } from '../lib/geometry';
  import { media } from '../lib/media.svelte';
  import { miniIsland } from '../lib/miniIsland.svelte';
  import { listen, onFileDrag, setHitRegion } from '../lib/platform';
  import { preferences } from '../lib/preferences.svelte';
  import {
    CLOSE_SPRING,
    CONTENT_HIDE_SPRING,
    CONTENT_IN_SPRING,
    CONTENT_OUT_SPRING,
    CONTENT_SCALE_IN_SPRING,
    HIDE_SPRING,
    OPEN_SPRING,
    Spring,
    type SpringParams,
  } from '../lib/spring';
  import { ui, type PanelTab } from '../lib/ui.svelte';
  import DropZones from './DropZones.svelte';
  import LiveActivity from './LiveActivity.svelte';
  import Panel from './Panel.svelte';

  type Mode = 'collapsed' | 'expanded' | 'dropping';

  let mode = $state<Mode>('collapsed');
  let tab = $state<PanelTab>('widgets');
  let dropZone = $state<string | null>(null);
  let panelSize = $state({ width: 640, height: 220 });
  let capsuleWidth = $state(0);
  let island: HTMLDivElement;

  let peeking = $state(true);
  let peekTimer: ReturnType<typeof setTimeout> | undefined;
  /** Set while the panel closes into the mini island; the hide delay starts once it is there. */
  let peekAfterClose = false;

  /** In "hide after a delay" mode, shows the mini island for the configured time. */
  function peek(): void {
    peeking = true;
    clearTimeout(peekTimer);
    peekTimer = setTimeout(() => (peeking = false), preferences.value.autoHideDelayMs);
  }

  const hasActivity = $derived(miniIsland.content !== null);

  /** The closed island is pulled into the top edge (zero height) when it has nothing to show or
   * the settings hide it. Pointing at the edge still opens it, and it then grows straight out of
   * the edge without showing the mini island. */
  const concealed = $derived.by(() => {
    if (mode !== 'collapsed') return false;
    // Stay in the edge until the setting is known, so "Hidden" never flashes the island.
    if (!preferences.loaded || !hasActivity) return true;
    const visibility = preferences.value.islandVisibility;
    return visibility === 'hidden' || (visibility === 'autoHide' && !peeking);
  });

  // What the mini island announces: files added to the tray, a widget's activity coming up
  // and, unless turned off in the settings, a new track, play or pause. A widget updating its
  // activity (a timer ticking, a count changing) does not bring the island out.
  const trayActivity = $derived(fileTray.files.length);
  const widgetActivity = $derived(miniIsland.content?.kind === 'widget' ? miniIsland.content.widgetId : '');
  const mediaActivity = $derived(
    [media.nowPlaying?.title, media.nowPlaying?.artist, media.nowPlaying?.playing].join('|'),
  );

  $effect(() => {
    void trayActivity;
    void widgetActivity;
    void preferences.value.islandVisibility;
    untrack(peek);
  });

  $effect(() => {
    void mediaActivity;
    untrack(() => {
      if (preferences.value.showOnMediaChange) peek();
    });
  });

  const target: IslandSize = $derived.by(() => {
    if (mode === 'expanded') return { ...panelSize, radius: EXPANDED_RADIUS };
    if (mode === 'dropping') return { ...DROP_SIZE, radius: EXPANDED_RADIUS };
    const width = Math.max(CAPSULE_MIN_WIDTH, Math.ceil(capsuleWidth));
    // Going into the edge it narrows while it rises, and comes back out the same way.
    if (concealed) return { width: Math.round(width * CONCEALED_WIDTH_SCALE), height: 0, radius: COLLAPSED_RADIUS };
    return { width, height: COLLAPSED_HEIGHT, radius: COLLAPSED_RADIUS };
  });

  // The island starts inside the top edge and comes out once the preferences are loaded.
  const width = new Spring(PILL_WIDTH);
  const height = new Spring(0);
  const radius = new Spring(0);

  /** A content layer fades in, sharpens and grows to full size on springs that run in the same
   * frames as the shape, so a change of mind mid-way carries on from where it is. */
  class Layer {
    element!: HTMLDivElement;
    readonly shown = new Spring(0, CONTENT_IN_SPRING, 0.01);
    readonly scale: Spring;

    constructor(hiddenScale: number) {
      this.scale = new Spring(hiddenScale, CONTENT_IN_SPRING, 0.01);
    }

    show(shown: boolean, scale: number, params: SpringParams, scaleParams = params): void {
      this.shown.setTarget(shown ? 1 : 0, params);
      this.scale.setTarget(scale, scaleParams);
    }

    step(dt: number): boolean {
      return [this.shown.step(dt), this.scale.step(dt)].every(Boolean);
    }

    render(): void {
      const shown = Math.min(1, Math.max(0, this.shown.value));
      this.element.style.opacity = shown.toFixed(3);
      this.element.style.filter = shown > 0.999 ? 'none' : `blur(${(8 * (1 - shown)).toFixed(2)}px)`;
      this.element.style.scale = this.scale.value.toFixed(4);
    }
  }

  const LIVE_HIDDEN_SCALE = 0.82;
  const PANEL_HIDDEN_SCALE = 0.94;
  const live = new Layer(LIVE_HIDDEN_SCALE);
  const panel = new Layer(PANEL_HIDDEN_SCALE);
  const drop = new Layer(PANEL_HIDDEN_SCALE);
  const layers = [live, panel, drop];

  let frame = 0;
  let lastTime = 0;
  let timestamps: number[] = [];

  function render(): void {
    island.style.width = `${width.value}px`;
    island.style.height = `${Math.max(0, height.value)}px`;
    const r = `${Math.max(0, radius.value)}px`;
    island.style.borderBottomLeftRadius = r;
    island.style.borderBottomRightRadius = r;
    // The shadow grows with the height, so the collapsed pill stays flat.
    const openness = Math.min(1, Math.max(0, (height.value - COLLAPSED_HEIGHT) / 80));
    island.style.setProperty('--shadow-alpha', (0.55 * openness).toFixed(3));
    for (const layer of layers) layer.render();
  }

  function tick(now: number): void {
    const dt = lastTime ? Math.min((now - lastTime) / 1000, 0.064) : 1 / 60;
    lastTime = now;
    timestamps.push(now);
    const settled = [width.step(dt), height.step(dt), radius.step(dt), ...layers.map((layer) => layer.step(dt))].every(
      Boolean,
    );
    render();
    if (settled) {
      frame = 0;
      lastTime = 0;
      debug.lastAnimation = frameStats(mode, timestamps);
      timestamps = [];
      if (peekAfterClose && mode === 'collapsed') {
        peekAfterClose = false;
        peek();
      }
      updateRegion();
    } else {
      frame = requestAnimationFrame(tick);
    }
  }

  /** Input region, which also clips what is drawn: it covers both the current and the target
   * size so animations are never cut off, plus room for the shadow (or a small hover margin
   * when flat). Only once the island has fully retracted into the edge does it shrink to the
   * strip along the top edge. */
  function updateRegion(): void {
    const w = Math.max(width.value, target.width);
    const h = Math.max(height.value, target.height);
    if (concealed && h < 1) {
      setHitRegion([
        {
          x: Math.floor((window.innerWidth - PILL_WIDTH) / 2 - HOVER_MARGIN),
          y: 0,
          width: PILL_WIDTH + HOVER_MARGIN * 2,
          height: HOT_ZONE_HEIGHT,
        },
      ]);
      return;
    }
    const x = (window.innerWidth - w) / 2;
    const open = h > COLLAPSED_HEIGHT + 1;
    const side = open ? SHADOW_MARGIN.side : HOVER_MARGIN;
    const bottom = open ? SHADOW_MARGIN.bottom : HOVER_MARGIN;
    setHitRegion([
      {
        x: Math.floor(x - side),
        y: 0,
        width: Math.ceil(w + side * 2),
        height: Math.ceil(h + bottom),
      },
    ]);
  }

  $effect(() => {
    const params = mode !== 'collapsed' ? OPEN_SPRING : concealed ? HIDE_SPRING : CLOSE_SPRING;
    width.setTarget(target.width, params);
    height.setTarget(target.height, params);
    radius.setTarget(target.radius, params);
    // Grow the input region before the island grows into it; it shrinks once settled.
    untrack(updateRegion);
    animate();
  });

  // Content comes in behind the shape and goes at once. The mini island's content shrinks into
  // the edge with it; under the open panel it only fades.
  $effect(() => {
    const showLive = mode === 'collapsed' && !concealed;
    if (showLive) live.show(true, 1, CONTENT_IN_SPRING, CONTENT_SCALE_IN_SPRING);
    else if (concealed) live.show(false, LIVE_HIDDEN_SCALE, CONTENT_HIDE_SPRING);
    else live.show(false, 1, CONTENT_OUT_SPRING);
    for (const [layer, shown] of [
      [panel, mode === 'expanded'],
      [drop, mode === 'dropping'],
    ] as const) {
      if (shown) layer.show(true, 1, CONTENT_IN_SPRING, CONTENT_SCALE_IN_SPRING);
      else layer.show(false, PANEL_HIDDEN_SCALE, CONTENT_OUT_SPRING);
    }
    untrack(animate);
  });

  function animate(): void {
    if (frame) return;
    timestamps = [];
    frame = requestAnimationFrame(tick);
  }

  $effect(() => {
    void concealed;
    untrack(() => updateRegion());
  });

  $effect(() => {
    ui.expanded = mode === 'expanded';
  });

  let openTimer: ReturnType<typeof setTimeout> | undefined;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let hovering = false;

  // While a widget's text field has the keyboard the island stays open, even if the cursor
  // leaves; once the keyboard is released it closes unless the cursor is over it.
  $effect(() => {
    const held = keyboard.held;
    untrack(() => {
      clearTimeout(closeTimer);
      if (!held && !hovering && mode === 'expanded') {
        closeTimer = setTimeout(() => setMode('collapsed'), CLOSE_DELAY_MS);
      }
    });
  });

  function setMode(next: Mode): void {
    clearTimeout(openTimer);
    clearTimeout(closeTimer);
    peekAfterClose = false;
    if (next === 'collapsed' && mode !== 'collapsed') {
      tab = 'widgets';
      void keyboard.set(false);
      // "Hide after a delay": the panel closes into the mini island, which stays for the
      // configured time before going into the edge. "Hidden" goes straight into the edge.
      if (preferences.value.islandVisibility === 'autoHide') {
        peeking = true;
        clearTimeout(peekTimer);
        peekAfterClose = true;
      }
    }
    mode = next;
  }

  /** The island opens only once the cursor has stayed on it for the configured time; leaving
   * earlier cancels it, so passing over the top edge does not open it by accident. */
  function pointerEnter(): void {
    hovering = true;
    clearTimeout(closeTimer);
    if (mode === 'collapsed') {
      openTimer = setTimeout(() => setMode('expanded'), preferences.value.openDelayMs);
    }
  }

  function pointerLeave(): void {
    hovering = false;
    clearTimeout(openTimer);
    if (mode === 'expanded' && !keyboard.held) closeTimer = setTimeout(() => setMode('collapsed'), CLOSE_DELAY_MS);
  }

  /** The island's shortcut opens and closes it. Opened that way it stays while the pointer is on
   * it and closes as usual when the pointer leaves; if the pointer never comes, it closes after
   * a while, so it does not keep covering the top of the screen. */
  function toggleFromShortcut(): void {
    if (mode === 'dropping') return;
    if (mode === 'expanded') {
      setMode('collapsed');
      return;
    }
    setMode('expanded');
    if (!hovering) closeTimer = setTimeout(() => setMode('collapsed'), SHORTCUT_CLOSE_DELAY_MS);
  }

  function zoneAt(x: number, y: number): string | null {
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-drop-zone]');
    return element?.dataset.dropZone ?? null;
  }

  /** After a drag ends over the island no pointerenter fires, so close on a timer unless the
   * user moves inside the island. */
  function closeSoon(): void {
    clearTimeout(closeTimer);
    closeTimer = setTimeout(() => setMode('collapsed'), 1600);
  }

  onMount(() => {
    let stopListening = () => {};
    // Files dragged out of the tray pass over the island first; that is not a drop.
    void onFileDrag({
      enter: (_paths, x, y) => {
        if (fileTray.draggingOut) return;
        setMode('dropping');
        dropZone = zoneAt(x, y);
      },
      over: (x, y) => {
        if (fileTray.draggingOut) return;
        dropZone = zoneAt(x, y);
      },
      drop: (paths, x, y) => {
        if (fileTray.draggingOut) return;
        const zone = zoneAt(x, y) ?? dropZone;
        dropZone = null;
        if (zone === 'tray') {
          void fileTray.add(paths);
          setMode('expanded');
          tab = 'tray';
          closeSoon();
        } else {
          setMode('collapsed');
        }
      },
      leave: () => {
        if (fileTray.draggingOut) return;
        dropZone = null;
        setMode('collapsed');
      },
    }).then((stop) => (stopListening = stop));

    let stopShortcut = () => {};
    void listen('hotkeys://toggle-island', toggleFromShortcut).then((stop) => (stopShortcut = stop));

    window.addEventListener('lizun:drag-out-finished', closeSoon);
    debug.setMode = setMode;
    debug.getMode = () => mode;
    return () => {
      stopListening();
      stopShortcut();
      window.removeEventListener('lizun:drag-out-finished', closeSoon);
    };
  });
</script>

<!-- The core fits the window to the primary monitor again when the displays or their scaling
     change; the input region is in physical pixels from the window's corner, so it is sent again. -->
<svelte:window onresize={updateRegion} />

<!-- Hover is tracked on a wrapper so that moving from the edge sensor into the growing island
     does not count as leaving. -->
<div
  class="hover-root"
  onpointerenter={pointerEnter}
  onpointerleave={pointerLeave}
  onpointermove={() => clearTimeout(closeTimer)}
  onpointerdown={() => void keyboard.set(false)}
  role="region"
  aria-label="Lizun"
>
  {#if concealed}
    <div class="sensor" style:width="{PILL_WIDTH + 2 * HOVER_MARGIN}px" style:height="{HOT_ZONE_HEIGHT}px"></div>
  {/if}
  <div class="island" bind:this={island}>
    <div class="layer live" class:visible={mode === 'collapsed' && !concealed} bind:this={live.element}>
      <LiveActivity active={mode === 'collapsed' && !concealed} bind:width={capsuleWidth} />
    </div>
    <div
      class="layer centered"
      class:visible={mode === 'expanded'}
      bind:this={panel.element}
      bind:clientWidth={panelSize.width}
      bind:clientHeight={panelSize.height}
    >
      <Panel bind:tab />
    </div>
    <div class="layer centered" class:visible={mode === 'dropping'} bind:this={drop.element}>
      <DropZones active={dropZone} />
    </div>
  </div>
</div>

<style>
  .hover-root {
    display: contents;
  }

  .sensor {
    position: absolute;
    top: 0;
    left: 50%;
    translate: -50% 0;
  }

  .island {
    position: absolute;
    top: 0;
    left: 50%;
    translate: -50% 0;
    width: 200px;
    height: 0;
    overflow: hidden;
    background: var(--island-bg);
    border-radius: 0 0 12px 12px;
    box-shadow: 0 14px 34px rgba(0, 0, 0, var(--shadow-alpha, 0));
  }

  /* Opacity, blur and scale are driven by springs in the script. */
  .layer {
    position: absolute;
    top: 0;
    opacity: 0;
    pointer-events: none;
    transform-origin: top center;
  }

  .layer.visible {
    pointer-events: auto;
  }

  .layer.live {
    left: 0;
    right: 0;
    height: 32px;
  }

  .layer.centered {
    left: 50%;
    translate: -50% 0;
    width: max-content;
  }
</style>
