<script lang="ts">
  import Alarm from 'phosphor-svelte/lib/Alarm';
  import BatteryHigh from 'phosphor-svelte/lib/BatteryHigh';
  import Bell from 'phosphor-svelte/lib/Bell';
  import CalendarBlank from 'phosphor-svelte/lib/CalendarBlank';
  import ChatCircle from 'phosphor-svelte/lib/ChatCircle';
  import CheckCircle from 'phosphor-svelte/lib/CheckCircle';
  import Clock from 'phosphor-svelte/lib/Clock';
  import Cloud from 'phosphor-svelte/lib/Cloud';
  import Coffee from 'phosphor-svelte/lib/Coffee';
  import DownloadSimple from 'phosphor-svelte/lib/DownloadSimple';
  import Envelope from 'phosphor-svelte/lib/Envelope';
  import FileIcon from 'phosphor-svelte/lib/File';
  import Fire from 'phosphor-svelte/lib/Fire';
  import Flag from 'phosphor-svelte/lib/Flag';
  import Headphones from 'phosphor-svelte/lib/Headphones';
  import Heart from 'phosphor-svelte/lib/Heart';
  import Hourglass from 'phosphor-svelte/lib/Hourglass';
  import Info from 'phosphor-svelte/lib/Info';
  import Lightning from 'phosphor-svelte/lib/Lightning';
  import ListChecks from 'phosphor-svelte/lib/ListChecks';
  import MapPin from 'phosphor-svelte/lib/MapPin';
  import Microphone from 'phosphor-svelte/lib/Microphone';
  import Moon from 'phosphor-svelte/lib/Moon';
  import Note from 'phosphor-svelte/lib/Note';
  import Package from 'phosphor-svelte/lib/Package';
  import Phone from 'phosphor-svelte/lib/Phone';
  import RecordIcon from 'phosphor-svelte/lib/Record';
  import Star from 'phosphor-svelte/lib/Star';
  import Sun from 'phosphor-svelte/lib/Sun';
  import Target from 'phosphor-svelte/lib/Target';
  import Timer from 'phosphor-svelte/lib/Timer';
  import Tray from 'phosphor-svelte/lib/Tray';
  import Trophy from 'phosphor-svelte/lib/Trophy';
  import UploadSimple from 'phosphor-svelte/lib/UploadSimple';
  import VideoCamera from 'phosphor-svelte/lib/VideoCamera';
  import Warning from 'phosphor-svelte/lib/Warning';
  import { fileTray } from '../lib/fileTray.svelte';
  import { CAPSULE_MAX_WIDTH } from '../lib/geometry';
  import { media } from '../lib/media.svelte';
  import { miniIsland } from '../lib/miniIsland.svelte';
  import type { ActivityIcon, ActivityPart } from '../lib/widgets/activities.svelte';

  /** Animations pause while the collapsed island is hidden behind the open panel. With `eyes`,
   * the capsule shows a pair of eyes while there is nothing else to show. */
  let {
    active,
    eyes,
    width = $bindable(0),
  }: { active: boolean; eyes: boolean; width?: number } = $props();

  const ICONS: Record<ActivityIcon, typeof Alarm> = {
    alarm: Alarm,
    battery: BatteryHigh,
    bell: Bell,
    calendar: CalendarBlank,
    chat: ChatCircle,
    'check-circle': CheckCircle,
    clock: Clock,
    cloud: Cloud,
    coffee: Coffee,
    download: DownloadSimple,
    envelope: Envelope,
    fire: Fire,
    flag: Flag,
    headphones: Headphones,
    heart: Heart,
    hourglass: Hourglass,
    info: Info,
    lightning: Lightning,
    'list-checks': ListChecks,
    'map-pin': MapPin,
    microphone: Microphone,
    moon: Moon,
    note: Note,
    package: Package,
    phone: Phone,
    record: RecordIcon,
    star: Star,
    sun: Sun,
    target: Target,
    timer: Timer,
    trophy: Trophy,
    upload: UploadSimple,
    'video-camera': VideoCamera,
    warning: Warning,
  };

  type Shown = {
    content: NonNullable<typeof miniIsland.content> | { kind: 'eyes' };
    nowPlaying: typeof media.nowPlaying;
    artwork: typeof media.artwork;
    tint: typeof media.tint;
    lastFile: (typeof fileTray.files)[number] | undefined;
  };

  /** What the capsule shows. When the activity ends it is kept, so the capsule goes into the
   * edge with it rather than emptying first. */
  let shown = $state.raw<Shown | null>(null);

  $effect.pre(() => {
    const content = miniIsland.content ?? (eyes ? { kind: 'eyes' as const } : null);
    if (!content) return;
    shown = {
      content,
      nowPlaying: media.nowPlaying,
      artwork: media.artwork,
      tint: media.tint,
      lastFile: fileTray.files.at(-1),
    };
  });
</script>

<!-- Windows has no camera notch, so the closed island is a capsule as wide as what it shows:
     something at each end and a line of text in the middle. -->
<div class="capsule" style:max-width="{CAPSULE_MAX_WIDTH}px" bind:clientWidth={width}>
  {#if shown?.content.kind === 'eyes'}
    <!-- They pop in each time the mini island comes out, then now and then look aside and blink. -->
    <div class="eyes" class:awake={active}><span></span><span></span></div>
  {:else if shown?.content.kind === 'media' && shown.nowPlaying}
    {@const { nowPlaying } = shown}
    {#if shown.artwork}
      <img class="art" src={shown.artwork} alt="" />
    {:else}
      <div class="art"></div>
    {/if}
    <span class="title">
      {nowPlaying.title}{#if nowPlaying.artist}<span class="secondary">{` · ${nowPlaying.artist}`}</span>{/if}
    </span>
    <div class="bars" class:playing={nowPlaying.playing && active} style:--tint={shown.tint}>
      <span></span><span></span><span></span><span></span>
    </div>
  {:else if shown?.content.kind === 'tray'}
    <Tray size={17} weight="fill" />
    {#if shown.lastFile}<span class="title">{shown.lastFile.name}</span>{/if}
    <span class="part">{shown.content.count}<FileIcon size={13} weight="fill" /></span>
  {:else if shown?.content.kind === 'widget'}
    {@const { leading, title, trailing } = shown.content.activity}
    {#if leading}{@render part(leading)}{/if}
    {#if title}<span class="title">{title}</span>{/if}
    {#if trailing}{@render part(trailing)}{/if}
  {/if}
</div>

{#snippet part(part: ActivityPart)}
  {@const Icon = part.icon && ICONS[part.icon]}
  <span class="part" style:color={part.color}>
    {#if Icon}<Icon size={17} weight="fill" />{/if}
    {#if part.text}<span class="text">{part.text}</span>{/if}
  </span>
{/snippet}

<style>
  /* Centered in the island, which springs to this width; while it grows the island clips it. */
  .capsule {
    position: absolute;
    top: 0;
    left: 50%;
    translate: -50% 0;
    display: flex;
    align-items: center;
    gap: 8px;
    width: max-content;
    height: 100%;
    padding: 0 13px 0 12px;
    box-sizing: border-box;
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }

  .art {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    object-fit: cover;
    background: var(--card-raised);
  }

  .title {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }

  .secondary {
    color: var(--text-secondary);
  }

  .part {
    display: flex;
    flex: none;
    align-items: center;
    gap: 3px;
    max-width: 72px;
    font-variant-numeric: tabular-nums;
  }

  .part .text {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .capsule > :global(svg),
  .bars {
    flex: none;
  }

  .bars {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 16px;
  }

  .bars span {
    width: 3px;
    height: 100%;
    border-radius: 2px;
    background: var(--tint);
    transform: scaleY(0.25);
    transition: transform 300ms ease;
  }

  .bars.playing span {
    animation: bounce 0.9s ease-in-out infinite alternate;
  }

  .bars.playing span:nth-child(2) {
    animation-duration: 0.7s;
    animation-delay: -0.3s;
  }

  .bars.playing span:nth-child(3) {
    animation-duration: 1.1s;
    animation-delay: -0.6s;
  }

  .bars.playing span:nth-child(4) {
    animation-duration: 0.8s;
    animation-delay: -0.15s;
  }

  @keyframes bounce {
    from {
      transform: scaleY(0.25);
    }
    to {
      transform: scaleY(1);
    }
  }

  /* With the capsule's padding the eyes make it 3.5 times as wide as it is high, like the logo. */
  .eyes {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 18px;
    width: 87px;
    height: 100%;
  }

  .eyes span {
    width: 7px;
    height: 11px;
    border-radius: 50%;
    background: var(--text);
  }

  /* Pure CSS on the compositor, and only while the mini island is out. The look and the blinks
   * run on cycles of different lengths, so they meet differently each time. */
  .eyes.awake {
    animation: look 17s cubic-bezier(0.45, 0, 0.2, 1) 2s infinite;
  }

  .eyes.awake span {
    animation:
      pop 0.42s ease-out backwards,
      blink 11s linear infinite;
  }

  .eyes.awake span:nth-child(2) {
    animation-delay: 0.07s, 0s;
  }

  @keyframes pop {
    from {
      scale: 0;
    }
    60% {
      scale: 1.25;
    }
    to {
      scale: 1;
    }
  }

  @keyframes look {
    0%,
    28%,
    56%,
    100% {
      translate: 0;
    }
    30%,
    40% {
      translate: -14px 0;
    }
    43%,
    53% {
      translate: 14px 0;
    }
  }

  /* One blink, and later a double one. */
  @keyframes blink {
    0%,
    35%,
    36.2%,
    88%,
    89.2%,
    90.2%,
    91.4%,
    100% {
      transform: none;
    }
    35.6%,
    88.6%,
    90.8% {
      transform: scaleY(0.08);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .eyes.awake,
    .eyes.awake span {
      animation: none;
    }
  }
</style>
