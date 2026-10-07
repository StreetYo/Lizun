import { invoke, listen } from './platform';

export interface NowPlaying {
  sourceApp: string;
  title: string;
  artist: string;
  album: string;
  playing: boolean;
  positionMs: number | null;
  durationMs: number | null;
  positionSampledAt: number | null;
  artworkKey: string | null;
  canToggle: boolean;
  canNext: boolean;
  canPrevious: boolean;
}

export type MediaCommand = 'togglePlayPause' | 'next' | 'previous';

class MediaStore {
  nowPlaying = $state<NowPlaying | null>(null);
  artwork = $state<string | null>(null);
  /** Artwork color for the equalizer bars. */
  tint = $state('rgb(255, 255, 255)');

  #artworkKey: string | null = null;
  #listeners = new Set<() => void>();
  #started = false;

  async start(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    await listen<NowPlaying | null>('media://changed', (update) => void this.#apply(update));
    await this.#apply(await invoke<NowPlaying | null>('media_snapshot'));
  }

  /** Called after every metadata or artwork change; used to push updates to widgets. */
  onChange(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  control(command: MediaCommand): Promise<void> {
    return invoke<void>('media_control', { command });
  }

  /** Playback position at `now`, advanced from the last sample while playing. */
  positionAt(now: number): number | null {
    const np = this.nowPlaying;
    if (!np || np.positionMs === null) return null;
    let position = np.positionMs;
    if (np.playing && np.positionSampledAt) position += now - np.positionSampledAt;
    return Math.max(0, Math.min(position, np.durationMs ?? position));
  }

  async #apply(update: NowPlaying | null): Promise<void> {
    this.nowPlaying = update;
    this.#notify();

    const key = update?.artworkKey ?? null;
    if (key === this.#artworkKey) return;
    this.#artworkKey = key;
    const artwork = key ? await invoke<string | null>('media_artwork', { key }) : null;
    if (this.#artworkKey !== key) return; // superseded by a newer cover
    this.artwork = artwork;
    this.tint = artwork ? await dominantColor(artwork) : 'rgb(255, 255, 255)';
    this.#notify();
  }

  #notify(): void {
    for (const listener of this.#listeners) listener();
  }
}

export const media = new MediaStore();

/** Average color of the cover, lifted so dark covers still give visible bars. */
async function dominantColor(src: string): Promise<string> {
  const image = new Image();
  image.src = src;
  try {
    await image.decode();
  } catch {
    return 'rgb(255, 255, 255)';
  }
  const size = 12;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.drawImage(image, 0, 0, size, size);
  const { data } = ctx.getImageData(0, 0, size, size);
  let r = 0;
  let g = 0;
  let b = 0;
  let weight = 0;
  for (let i = 0; i < data.length; i += 4) {
    const max = Math.max(data[i], data[i + 1], data[i + 2]);
    const min = Math.min(data[i], data[i + 1], data[i + 2]);
    const w = 1 + (max - min) / 32; // favour saturated pixels over greys
    r += data[i] * w;
    g += data[i + 1] * w;
    b += data[i + 2] * w;
    weight += w;
  }
  [r, g, b] = [r / weight, g / weight, b / weight];
  const lift = Math.max(1, 150 / Math.max(r, g, b, 1));
  return `rgb(${Math.min(255, Math.round(r * lift))}, ${Math.min(255, Math.round(g * lift))}, ${Math.min(255, Math.round(b * lift))})`;
}
