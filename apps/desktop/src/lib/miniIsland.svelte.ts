import { fileTray } from './fileTray.svelte';
import { media } from './media.svelte';
import { activities, type LiveActivity } from './widgets/activities.svelte';
import { widgetLayout } from './widgets/layout.svelte';
import { widgetRegistry } from './widgets/registry.svelte';

export type MiniContent =
  | { kind: 'media' }
  | { kind: 'tray'; count: number }
  | { kind: 'widget'; widgetId: string; activity: LiveActivity };

/** What the closed island shows, one thing at a time: the current track, then files in the
 * tray, then widget activities in the order the widgets have in the island (so the user sets
 * their priority by moving widgets). Hidden widgets show nothing. Null when there is nothing to
 * show, and the mini island then goes into the edge. */
class MiniIsland {
  content = $derived.by((): MiniContent | null => {
    if (media.nowPlaying) return { kind: 'media' };
    if (fileTray.files.length) return { kind: 'tray', count: fileTray.files.length };
    for (const entry of widgetLayout(widgetRegistry.widgets)) {
      const activity = activities.byWidget[entry.id];
      if (entry.enabled && activity) return { kind: 'widget', widgetId: entry.id, activity };
    }
    return null;
  });
}

export const miniIsland = new MiniIsland();
