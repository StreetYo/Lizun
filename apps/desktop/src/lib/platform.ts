/**
 * Thin layer over the Tauri APIs. Outside Tauri (plain browser preview) calls fall back to
 * mocks, so the island can be designed and screenshot-tested in any browser.
 */
import { convertFileSrc, invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { mockInvoke } from './mocks';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return isTauri ? tauriInvoke<T>(command, args) : mockInvoke<T>(command, args);
}

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  return tauriListen<T>(event, (e) => handler(e.payload));
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Limits mouse input to these rectangles; clicks elsewhere reach the windows below. */
export function setHitRegion(rects: Rect[]): void {
  if (!isTauri) return;
  invoke('island_set_hit_region', { rects }).catch((err) => console.warn('hit region', err));
}

export interface FileDragHandlers {
  enter(paths: string[], x: number, y: number): void;
  over(x: number, y: number): void;
  drop(paths: string[], x: number, y: number): void;
  leave(): void;
}

/** File drags from Explorer. Positions are converted to CSS pixels. */
export async function onFileDrag(handlers: FileDragHandlers): Promise<() => void> {
  if (!isTauri) return () => {};
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    const scale = window.devicePixelRatio || 1;
    switch (payload.type) {
      case 'enter':
        handlers.enter(payload.paths, payload.position.x / scale, payload.position.y / scale);
        break;
      case 'over':
        handlers.over(payload.position.x / scale, payload.position.y / scale);
        break;
      case 'drop':
        handlers.drop(payload.paths, payload.position.x / scale, payload.position.y / scale);
        break;
      case 'leave':
        handlers.leave();
        break;
    }
  });
}

/** URL of a local file the backend has allowed through the asset protocol. */
export function fileUrl(path: string): string {
  return isTauri ? convertFileSrc(path) : '';
}

/** Base URL of the protocol that serves widget files (see src-tauri/src/widgets.rs). */
export const WIDGET_ORIGIN = navigator.userAgent.includes('Windows')
  ? 'http://lizun-widget.localhost'
  : 'lizun-widget://localhost';
