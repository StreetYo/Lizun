import { startDrag } from '@crabnebula/tauri-plugin-drag';
import { fileUrl, invoke, isTauri } from './platform';

const STORAGE_KEY = 'lizun.fileTray';
const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'avif']);

export interface TrayFile {
  path: string;
  name: string;
  extension: string;
  preview: string | null;
}

/** Files dropped onto the island. Only paths are kept; the files stay where they are. */
class FileTray {
  files = $state<TrayFile[]>([]);
  /** True while a file is being dragged out, so the island ignores its own drag. */
  draggingOut = false;

  /** Starts a system drag of the file, so it can be dropped into any app. */
  async dragOut(file: TrayFile): Promise<void> {
    if (!isTauri || this.draggingOut) return;
    this.draggingOut = true;
    try {
      await startDrag({ item: [file.path], icon: await dragIcon(file), mode: 'copy' });
    } catch (err) {
      console.warn('drag out failed', err);
    } finally {
      this.draggingOut = false;
    }
  }

  async restore(): Promise<void> {
    let saved: string[] = [];
    try {
      saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]');
    } catch {
      saved = [];
    }
    if (saved.length) await this.add(saved);
  }

  async add(paths: string[]): Promise<void> {
    const fresh = paths.filter((path) => !this.files.some((file) => file.path === path));
    if (!fresh.length) return;
    await invoke('file_tray_allow', { paths: fresh });
    this.files = [...this.files, ...fresh.map(describeFile)];
    this.#save();
  }

  remove(path: string): void {
    this.files = this.files.filter((file) => file.path !== path);
    this.#save();
  }

  clear(): void {
    this.files = [];
    this.#save();
  }

  #save(): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.files.map((file) => file.path)));
    } catch {
      // Storage can be unavailable; the tray then simply does not survive a restart.
    }
  }
}

/** PNG shown under the cursor while dragging: the image itself, or a labelled file card. */
async function dragIcon(file: TrayFile): Promise<string> {
  const size = 64;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext('2d')!;
  if (file.preview) {
    try {
      // Drawing an <img> from the asset origin would taint the canvas; a fetched blob does not.
      const bitmap = await createImageBitmap(await (await fetch(file.preview)).blob());
      const scale = Math.min(size / bitmap.width, size / bitmap.height);
      const w = bitmap.width * scale;
      const h = bitmap.height * scale;
      ctx.drawImage(bitmap, (size - w) / 2, (size - h) / 2, w, h);
      bitmap.close();
      return canvas.toDataURL('image/png');
    } catch {
      // Fall through to the generic card.
    }
  }
  ctx.fillStyle = '#26252a';
  ctx.beginPath();
  ctx.roundRect(8, 2, 48, 60, 8);
  ctx.fill();
  ctx.fillStyle = '#0a84ff';
  ctx.font = 'bold 13px sans-serif';
  ctx.textAlign = 'center';
  ctx.fillText((file.extension || 'file').slice(0, 4).toUpperCase(), size / 2, 38);
  return canvas.toDataURL('image/png');
}

/** Name, extension and, for pictures, the preview URL of a file the backend has allowed. */
export function describeFile(path: string): TrayFile {
  const name = path.split(/[\\/]/).pop() || path;
  const dot = name.lastIndexOf('.');
  const extension = dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
  return { path, name, extension, preview: IMAGE_EXTENSIONS.has(extension) ? fileUrl(path) : null };
}

export const fileTray = new FileTray();
