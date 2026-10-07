/** Diagnostics exposed as `window.__LIZUN_DEBUG__` in development builds. */

export interface AnimationStats {
  mode: string;
  frames: number;
  durationMs: number;
  avgFrameMs: number;
  p95FrameMs: number;
  maxFrameMs: number;
}

export const debug = {
  lastAnimation: null as AnimationStats | null,
  widgetReports: {} as Record<string, unknown>,
  setMode: (_mode: 'collapsed' | 'expanded' | 'dropping') => {},
  getMode: (): string => '',
};

if (import.meta.env.DEV) {
  (window as unknown as { __LIZUN_DEBUG__: typeof debug }).__LIZUN_DEBUG__ = debug;
}

export function frameStats(mode: string, timestamps: number[]): AnimationStats | null {
  if (timestamps.length < 2) return null;
  const deltas = timestamps.slice(1).map((t, i) => t - timestamps[i]);
  const sorted = [...deltas].sort((a, b) => a - b);
  const round = (n: number) => Math.round(n * 100) / 100;
  return {
    mode,
    frames: deltas.length,
    durationMs: round(timestamps[timestamps.length - 1] - timestamps[0]),
    avgFrameMs: round(deltas.reduce((sum, d) => sum + d, 0) / deltas.length),
    p95FrameMs: round(sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))]),
    maxFrameMs: round(sorted[sorted.length - 1]),
  };
}
