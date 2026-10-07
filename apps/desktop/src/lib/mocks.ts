/** Fake backend for previewing the island in a plain browser. */

function sampleArtwork(): string {
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 160;
  const ctx = canvas.getContext('2d')!;
  const sky = ctx.createLinearGradient(0, 0, 0, 160);
  sky.addColorStop(0, '#5aa4d6');
  sky.addColorStop(1, '#e8c79a');
  ctx.fillStyle = sky;
  ctx.fillRect(0, 0, 160, 160);
  ctx.fillStyle = '#1b1b24';
  ctx.beginPath();
  ctx.ellipse(80, 112, 34, 30, 0, 0, Math.PI * 2);
  ctx.ellipse(80, 72, 22, 20, 0, 0, Math.PI * 2);
  ctx.fill();
  return canvas.toDataURL('image/png');
}

const startedAt = Date.now();
let mockPreferences: unknown = {
  islandVisibility: 'always',
  autoHideDelayMs: 1000,
  showOnMediaChange: true,
  openDelayMs: 100,
  browserMedia: true,
  browserMediaPort: 6869,
};

/** Widgets the preview lists; empty, but a test can push manifests into it. */
export const mockWidgets: { id: string; settings: { key: string; default: unknown }[] }[] = [];
const mockWidgetSettings: Record<string, Record<string, unknown>> = {};

function mockSettingsOf(widgetId: string): Record<string, unknown> {
  const widget = mockWidgets.find((w) => w.id === widgetId);
  const defaults = Object.fromEntries((widget?.settings ?? []).map((s) => [s.key, s.default]));
  return { ...defaults, ...mockWidgetSettings[widgetId] };
}

export async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case 'preferences_get':
      return mockPreferences as T;
    case 'preferences_set':
      mockPreferences = args?.preferences;
      return mockPreferences as T;
    case 'media_snapshot':
      return {
        sourceApp: 'Spotify.exe',
        title: 'My Seaside Morning – Vintage Jazz',
        artist: 'Easy Cat Jazz',
        album: 'Mornings',
        playing: true,
        positionMs: 78_000,
        durationMs: 244_000,
        positionSampledAt: startedAt,
        artworkKey: 'mock',
        canToggle: true,
        canNext: true,
        canPrevious: true,
      } as T;
    case 'media_artwork':
      return sampleArtwork() as T;
    case 'media_players':
      return [
        { key: 'browser:1:7:0', sourceApp: 'youtube.com', title: 'Sample video', artist: 'Channel', playing: false, shown: false },
        { key: 'system:Spotify.exe', sourceApp: 'Spotify.exe', title: 'My Seaside Morning – Vintage Jazz', artist: 'Easy Cat Jazz', playing: true, shown: true },
      ] as T;
    case 'widgets_list':
    case 'widgets_reload':
      return mockWidgets as T;
    case 'widget_settings_all':
      return Object.fromEntries(mockWidgets.map((w) => [w.id, mockSettingsOf(w.id)])) as T;
    case 'widget_settings_set': {
      const { widgetId, key, value } = args as { widgetId: string; key: string; value: unknown };
      mockWidgetSettings[widgetId] = { ...mockWidgetSettings[widgetId], [key]: value };
      return mockSettingsOf(widgetId) as T;
    }
    case 'widget_settings_reset':
      delete mockWidgetSettings[(args as { widgetId: string }).widgetId];
      return mockSettingsOf((args as { widgetId: string }).widgetId) as T;
    case 'app_info':
      return { version: 'preview', widgetsDir: null } as T;
    case 'browser_media_status':
      return { enabled: true, port: 6869, listening: true, error: null, browsers: [] } as T;
    default:
      return undefined as T;
  }
}
