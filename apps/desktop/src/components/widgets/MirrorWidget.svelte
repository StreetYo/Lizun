<script lang="ts">
  import Webcam from 'phosphor-svelte/lib/Webcam';
  import { t } from '../../lib/i18n.svelte';
  import { ui } from '../../lib/ui.svelte';

  let stream = $state<MediaStream | null>(null);
  let error = $state<string | null>(null);
  let video = $state<HTMLVideoElement>();

  async function toggle(): Promise<void> {
    if (stream) return stop();
    error = null;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { width: 320, height: 320 }, audio: false });
    } catch (err) {
      error = err instanceof Error ? err.name : t('mirror.unavailable');
    }
  }

  function stop(): void {
    stream?.getTracks().forEach((track) => track.stop());
    stream = null;
  }

  $effect(() => {
    if (video && stream) video.srcObject = stream;
  });

  // Never keep the camera on once the island closes.
  $effect(() => {
    if (!ui.expanded) stop();
  });
</script>

<section class="mirror">
  <button class="circle" onclick={toggle} title={stream ? t('mirror.turnOff') : t('mirror.showCamera')}>
    {#if stream}
      <video bind:this={video} autoplay muted playsinline></video>
    {:else}
      <Webcam size={26} weight="fill" />
      <span>{error ?? t('mirror.label')}</span>
    {/if}
  </button>
</section>

<style>
  .mirror {
    display: grid;
    place-items: center;
    width: 132px;
  }

  .circle {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    width: 116px;
    height: 116px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--card);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 600;
    transition: transform 150ms ease;
  }

  .circle:active {
    transform: scale(0.96);
  }

  video {
    width: 100%;
    height: 100%;
    object-fit: cover;
    transform: scaleX(-1);
  }
</style>
