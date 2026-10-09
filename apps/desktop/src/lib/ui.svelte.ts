/** UI state shared between the island shell and the widgets inside it. */
export const ui = $state({
  /** True while the full panel is open; widgets pause timers and the camera otherwise. */
  expanded: false,
});

/** Tabs of the open island. */
export type PanelTab = 'widgets' | 'tray' | 'clipboard';
