import { invoke, listen } from './platform';

/**
 * The island never takes focus, so typing goes to the app the user works in. The exception is
 * a text field in a widget: while one is in use the island takes the keyboard, and afterwards
 * the focus goes back to the previously active window.
 */
class IslandKeyboard {
  held = $state(false);

  #queue = Promise.resolve();
  #onRelease = new Set<() => void>();
  #started = false;

  /** Releases the keyboard when the user activates another window while typing in the island. */
  async start(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    await listen('island://deactivated', () => void this.set(false));
  }

  /** Takes or releases the keyboard; calls are applied in order. */
  set(take: boolean): Promise<void> {
    this.#queue = this.#queue
      .then(async () => {
        if (take === this.held) return;
        this.held = take;
        if (!take) for (const listener of this.#onRelease) listener();
        await invoke('island_keyboard', { take });
      })
      .catch((err) => console.warn('could not change the keyboard focus', err));
    return this.#queue;
  }

  /** Called whenever the island gives the keyboard back, whoever asked for it. */
  onRelease(listener: () => void): () => void {
    this.#onRelease.add(listener);
    return () => this.#onRelease.delete(listener);
  }
}

export const keyboard = new IslandKeyboard();
