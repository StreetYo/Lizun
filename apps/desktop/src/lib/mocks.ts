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
  language: 'system',
  islandVisibility: 'always',
  autoHideDelayMs: 1000,
  showOnMediaChange: true,
  openDelayMs: 100,
  browserMedia: true,
  browserMediaPort: 6869,
  widgets: [],
  clipboardHistory: true,
  clipboardLifetimeHours: 24,
  clipboardLimitEnabled: true,
  clipboardLimit: 50,
};

/** Clipboard history for the preview: texts of several lengths and a picture with no file. */
let mockClipboard = [
  {
    id: 6,
    copiedAt: startedAt - 5_000,
    kind: 'files',
    files: [
      { path: 'C:\\Users\\you\\Pictures\\beach.jpg', folder: false, missing: false },
      { path: 'C:\\Users\\you\\Documents\\report.pdf', folder: false, missing: false },
      { path: 'C:\\Users\\you\\Projects', folder: true, missing: false },
      { path: 'C:\\Users\\you\\notes.txt', folder: false, missing: false },
    ],
    count: 7,
  },
  {
    id: 5,
    copiedAt: startedAt - 60_000,
    kind: 'files',
    files: [{ path: 'C:\\Users\\you\\Downloads\\setup.exe', folder: false, missing: false }],
    count: 1,
  },
  { id: 4, copiedAt: startedAt - 20_000, kind: 'text', text: 'npm run build -- --watch', cut: false },
  { id: 3, copiedAt: startedAt - 7 * 60_000, kind: 'picture', path: '', width: 1920, height: 1080 },
  {
    id: 2,
    copiedAt: startedAt - 3 * 3_600_000,
    kind: 'text',
    text: 'Hi Alex,\n\nThanks for the notes from yesterday. I went through the design review comments and updated the mockups; the new version is in the shared folder.',
    cut: true,
  },
  { id: 1, copiedAt: startedAt - 26 * 3_600_000, kind: 'text', text: 'https://github.com/lizun-app/lizun', cut: false },
];

/** Widgets the preview lists; empty, but a test can push manifests into it. */
export const mockWidgets: { id: string; settings: { key: string; default: unknown }[] }[] = [];
const mockWidgetSettings: Record<string, Record<string, unknown>> = {};
/** Mirrors `settings()` in src-tauri/src/calendar.rs. */
const mockBuiltInSettings = {
  'builtin:calendar': [
    { key: 'icsUrl', type: 'text', label: 'Calendar link or file', description: 'The secret iCal address of a Google, Outlook or iCloud calendar, or the path to an .ics file. It stays on this computer.', default: '', maxLength: 500, placeholder: 'https://…/basic.ics or C:\\…\\calendar.ics', file: { name: 'Calendar', extensions: ['ics'] } },
    { key: 'endedToday', type: 'select', label: 'Events that ended today', description: 'Earlier days always show their events, dimmed.', default: 'dim', options: [{ value: 'dim', label: 'Show dimmed' }, { value: 'hide', label: 'Hide' }] },
    { key: 'refreshMinutes', type: 'number', label: 'Refresh every', description: 'Google updates the feed itself only every few hours. Files are read again as soon as they change.', default: 15, min: 5, max: 240, step: 5, unit: 'min' },
  ],
};

/** Events of the week around today for the preview: a meeting now, one later, an all-day one. */
function mockCalendar(): unknown {
  const at = (days: number, hours: number, minutes = 0) => {
    const date = new Date();
    date.setDate(date.getDate() + days);
    date.setHours(hours, minutes, 0, 0);
    return date.getTime();
  };
  const hour = new Date().getHours();
  const events = [
    { title: 'Conference', location: '', start: at(0, 0), end: at(1, 0), allDay: true },
    { title: 'Design review', location: 'Room 4', start: at(0, hour), end: at(0, hour + 1), allDay: false },
    { title: 'Lunch with Alex', location: '', start: at(0, hour + 2), end: at(0, hour + 3), allDay: false },
    { title: 'Weekly planning', location: '', start: at(1, 10), end: at(1, 11), allDay: false },
    { title: 'Dentist', location: '', start: at(3, 16, 30), end: at(3, 17), allDay: false },
  ];
  const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return { configured: true, loading: false, events, error: null, updatedAt: Date.now(), timeZone };
}
let mockPending: { id: string } & Record<string, unknown> | null = null;

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
    case 'i18n_system_language':
      return navigator.language.split('-')[0] as T;
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
    case 'widget_settings_built_in':
      return mockBuiltInSettings as T;
    case 'widget_settings_all':
      return Object.fromEntries(mockWidgets.map((w) => [w.id, mockSettingsOf(w.id)])) as T;
    case 'calendar_snapshot':
      return mockCalendar() as T;
    case 'widget_settings_set': {
      const { widgetId, key, value } = args as { widgetId: string; key: string; value: unknown };
      mockWidgetSettings[widgetId] = { ...mockWidgetSettings[widgetId], [key]: value };
      return mockSettingsOf(widgetId) as T;
    }
    case 'widget_settings_reset':
      delete mockWidgetSettings[(args as { widgetId: string }).widgetId];
      return mockSettingsOf((args as { widgetId: string }).widgetId) as T;
    case 'widgets_install_prepare': {
      // A link or path with "fail" in it shows the error path.
      const source = args?.source as { url?: string; path?: string };
      await new Promise((resolve) => setTimeout(resolve, 900));
      if ((source.url ?? source.path ?? '').includes('fail')) {
        throw 'The manifest is invalid: unknown permission "files:write"';
      }
      mockPending = {
        id: 'com.example.clock',
        name: 'World Clock',
        version: '1.2.0',
        author: 'Example',
        description: 'Time in the cities you choose',
        entry: 'index.html',
        width: 240,
        permissions: ['storage', 'activity'],
        network: ['https://worldtimeapi.org'],
        settings: [],
        source: 'user',
      };
      const replaces = mockWidgets.find((w) => w.id === mockPending!.id) as { version?: string } | undefined;
      return { token: 1, manifest: mockPending, origin: source.url ?? source.path, replaces: replaces?.version ?? null } as T;
    }
    case 'widgets_install_confirm': {
      const index = mockWidgets.findIndex((w) => w.id === mockPending?.id);
      if (mockPending) mockWidgets.splice(index < 0 ? mockWidgets.length : index, index < 0 ? 0 : 1, mockPending as never);
      mockPending = null;
      return mockWidgets as T;
    }
    case 'widgets_install_cancel':
      mockPending = null;
      return undefined as T;
    case 'widgets_uninstall':
      mockWidgets.splice(mockWidgets.findIndex((w) => w.id === (args as { id: string }).id), 1);
      return mockWidgets as T;
    case 'app_info':
      return { version: 'preview', widgetsDir: null } as T;
    case 'clipboard_history':
      return mockClipboard as T;
    case 'clipboard_copy':
      return undefined as T;
    case 'clipboard_remove':
      mockClipboard = mockClipboard.filter((entry) => entry.id !== (args as { id: number }).id);
      return undefined as T;
    case 'clipboard_clear':
      mockClipboard = [];
      return undefined as T;
    case 'browser_media_status':
      return { enabled: true, port: 6869, listening: true, error: null, browsers: [] } as T;
    default:
      return undefined as T;
  }
}
