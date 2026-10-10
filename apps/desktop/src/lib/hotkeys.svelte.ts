/**
 * Global shortcuts (see src-tauri/src/hotkeys.rs). A shortcut is written like `Ctrl+Alt+Space`:
 * modifiers in the order Ctrl, Alt, Shift, Win, then a key by its `KeyboardEvent.code` name or
 * `MouseBack` / `MouseForward` for the side mouse buttons. The core decides which shortcuts are
 * allowed; this file only shows them and turns key presses into them.
 */
import { t } from './i18n.svelte';
import { invoke, listen } from './platform';
import type { HotkeyAction } from './preferences.svelte';

/** Why the core refuses a shortcut (see `Refusal` in src-tauri/src/hotkeys.rs). */
export type Refusal = 'key' | 'modifier' | 'oneModifier' | 'altGr' | 'system' | 'apps';

class HotkeyStatus {
  /** Actions whose shortcut another app or Windows already has. */
  taken = $state<HotkeyAction[]>([]);
  #started = false;

  async start(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    await listen<HotkeyAction[]>('hotkeys://status', (taken) => (this.taken = taken));
    this.taken = (await invoke<HotkeyAction[]>('hotkeys_status')) ?? [];
  }
}

export const hotkeyStatus = new HotkeyStatus();

const MODIFIER_CODES = new Set([
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'MetaLeft',
  'MetaRight',
  'OSLeft',
  'OSRight',
]);

const SIGNS: Record<string, string> = {
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Backquote: '`',
  Comma: ',',
  Period: '.',
  Slash: '/',
  IntlBackslash: '\\',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  PageUp: 'Page Up',
  PageDown: 'Page Down',
  NumpadAdd: 'Num +',
  NumpadSubtract: 'Num -',
  NumpadMultiply: 'Num *',
  NumpadDivide: 'Num /',
  NumpadDecimal: 'Num .',
};

/** How a key is written on the keyboard: `KeyK` is K, `ArrowLeft` is ←. */
function keyLabel(code: string): string {
  if (code === 'Space') return t('settings.shortcuts.keys.space');
  if (code === 'MouseBack') return t('settings.shortcuts.keys.mouseBack');
  if (code === 'MouseForward') return t('settings.shortcuts.keys.mouseForward');
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^Numpad\d$/.test(code)) return `Num ${code.slice(6)}`;
  return SIGNS[code] ?? code;
}

/** The keys of a shortcut as they are written on the keyboard, one per keycap. */
export function shortcutParts(shortcut: string): string[] {
  const parts = shortcut.split('+');
  return [...parts.slice(0, -1), keyLabel(parts[parts.length - 1])];
}

/** The modifiers held during an event, in the order shortcuts list them. */
export function modifiersOf(event: KeyboardEvent | MouseEvent): string[] {
  return [
    event.ctrlKey && 'Ctrl',
    event.altKey && 'Alt',
    event.shiftKey && 'Shift',
    event.metaKey && 'Win',
  ].filter((name): name is string => Boolean(name));
}

/** The shortcut a key press makes, or null when the key is a modifier, which only starts one. */
export function shortcutOfKey(event: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(event.code)) return null;
  return [...modifiersOf(event), event.code].join('+');
}

/** The shortcut a mouse press makes; only the side buttons make one. */
export function shortcutOfMouse(event: MouseEvent): string | null {
  const button = event.button === 3 ? 'MouseBack' : event.button === 4 ? 'MouseForward' : null;
  return button && [...modifiersOf(event), button].join('+');
}
