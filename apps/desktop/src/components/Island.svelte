<script lang="ts">
  import { onMount } from 'svelte';
  import { debug, frameStats } from '../lib/debug';
  import { fileTray } from '../lib/fileTray.svelte';
  import {
    CLOSE_DELAY_MS,
    COLLAPSED_HEIGHT,
    COLLAPSED_RADIUS,
    DROP_SIZE,
    EAR_WIDTH,
    EXPANDED_RADIUS,
    HOVER_MARGIN,
    OPEN_DELAY_MS,
    PILL_WIDTH,
    SHADOW_MARGIN,
    type IslandSize,
  } from '../lib/geometry';
  import { media } from '../lib/media.svelte';
  import { onFileDrag, setHitRegion } from '../lib/platform';
  import { CLOSE_SPRING, OPEN_SPRING, Spring } from '../lib/spring';
  import { ui } from '../lib/ui.svelte';
  import DropZones from './DropZones.svelte';
  import LiveActivity from './LiveActivity.svelte';
  import Panel from './Panel.svelte';

  type Mode = 'collapsed' | 'expanded' | 'dropping';

  let mode = $state<Mode>('collapsed');
  let tab = $state<'widgets' | 'tray'>('widgets');
  let dropZone = $state<string | null>(null);
  let panelSize = $state({ width: 640, height: 220 });
  let island: HTMLDivElement;

  const hasActivity = $derived(media.nowPlaying !== null || fileTray.files.length > 0);
  const target: IslandSize = $derived.by(() => {
    if (mode === 'expanded') return { ...panelSize, radius: EXPANDED_RADIUS };
    if (mode === 'dropping') return { ...DROP_SIZE, radius: EXPANDED_RADIUS };
    const width = hasActivity ? PILL_WIDTH + 2 * EAR_WIDTH : PILL_WIDTH;
    return { width, height: COLLAPSED_HEIGHT, radius: COLLAPSED_RADIUS };
  });

  const width = new Spring(PILL_WIDTH);
  const height = new Spring(COLLAPSED_HEIGHT);
  const radius = new Spring(COLLAPSED_RADIUS);

  let frame = 0;
  let lastTime = 0;
  let timestamps: number[] = [];

  function render(): void {
    island.style.width = `${width.value}px`;
    island.style.height = `${height.value}px`;
    const r = `${Math.max(0, radius.value)}px`;
    island.style.borderBottomLeftRadius = r;
    island.style.borderBottomRightRadius = r;
    // The shadow grows with the height, so the collapsed pill stays flat.
    const openness = Math.min(1, Math.max(0, (height.value - COLLAPSED_HEIGHT) / 80));
    island.style.setProperty('--shadow-alpha', (0.55 * openness).toFixed(3));
  }

  function tick(now: number): void {
    const dt = lastTime ? Math.min((now - lastTime) / 1000, 0.064) : 1 / 60;
    lastTime = now;
    timestamps.push(now);
    const settled = [width.step(dt), height.step(dt), radius.step(dt)].every(Boolean);
    render();
    if (settled) {
      frame = 0;
      lastTime = 0;
      debug.lastAnimation = frameStats(mode, timestamps);
      timestamps = [];
      updateRegion();
    } else {
      frame = requestAnimationFrame(tick);
    }
  }

  /** Input region: the island plus room for its shadow, or a small hover margin when flat. */
  function updateRegion(next?: IslandSize): void {
    const w = Math.max(width.value, next?.width ?? 0);
    const h = Math.max(height.value, next?.height ?? 0);
    const open = h > COLLAPSED_HEIGHT + 1;
    const side = open ? SHADOW_MARGIN.side : HOVER_MARGIN;
    const bottom = open ? SHADOW_MARGIN.bottom : HOVER_MARGIN;
    setHitRegion([
      {
        x: Math.floor((window.innerWidth - w) / 2 - side),
        y: 0,
        width: Math.ceil(w + side * 2),
        height: Math.ceil(h + bottom),
      },
    ]);
  }

  $effect(() => {
    const params = mode === 'collapsed' ? CLOSE_SPRING : OPEN_SPRING;
    width.setTarget(target.width, params);
    height.setTarget(target.height, params);
    radius.setTarget(target.radius, params);
    // Grow the input region before the island grows into it; it shrinks once settled.
    updateRegion(target);
    if (!frame) {
      timestamps = [];
      frame = requestAnimationFrame(tick);
    }
  });

  $effect(() => {
    ui.expanded = mode === 'expanded';
  });

  let openTimer: ReturnType<typeof setTimeout> | undefined;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  function setMode(next: Mode): void {
    clearTimeout(openTimer);
    clearTimeout(closeTimer);
    if (next === 'collapsed') tab = 'widgets';
    mode = next;
  }

  function pointerEnter(): void {
    clearTimeout(closeTimer);
    if (mode === 'collapsed') openTimer = setTimeout(() => setMode('expanded'), OPEN_DELAY_MS);
  }

  function pointerLeave(): void {
    clearTimeout(openTimer);
    if (mode === 'expanded') closeTimer = setTimeout(() => setMode('collapsed'), CLOSE_DELAY_MS);
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

    window.addEventListener('lizun:drag-out-finished', closeSoon);
    debug.setMode = setMode;
    debug.getMode = () => mode;
    return () => {
      stopListening();
      window.removeEventListener('lizun:drag-out-finished', closeSoon);
    };
  });
</script>

<div
  class="island"
  bind:this={island}
  onpointerenter={pointerEnter}
  onpointerleave={pointerLeave}
  onpointermove={() => clearTimeout(closeTimer)}
  role="region"
  aria-label="Lizun"
>
  <div class="layer live" class:visible={mode === 'collapsed'}>
    <LiveActivity active={mode === 'collapsed'} />
  </div>
  <div
    class="layer centered"
    class:visible={mode === 'expanded'}
    bind:clientWidth={panelSize.width}
    bind:clientHeight={panelSize.height}
  >
    <Panel bind:tab />
  </div>
  <div class="layer centered" class:visible={mode === 'dropping'}>
    <DropZones active={dropZone} />
  </div>
</div>

<style>
  .island {
    position: absolute;
    top: 0;
    left: 50%;
    translate: -50% 0;
    width: 200px;
    height: 32px;
    overflow: hidden;
    background: var(--island-bg);
    border-radius: 0 0 12px 12px;
    box-shadow: 0 14px 34px rgba(0, 0, 0, var(--shadow-alpha, 0));
  }

  .layer {
    position: absolute;
    top: 0;
    opacity: 0;
    filter: blur(8px);
    pointer-events: none;
    transition:
      opacity 120ms ease,
      filter 160ms ease,
      scale 160ms ease;
  }

  .layer.visible {
    opacity: 1;
    filter: none;
    pointer-events: auto;
    transition:
      opacity 240ms ease 70ms,
      filter 320ms ease 70ms,
      scale 380ms cubic-bezier(0.2, 0.8, 0.2, 1) 40ms;
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
    scale: 0.96;
    transform-origin: top center;
  }

  .layer.centered.visible {
    scale: 1;
  }
</style>
