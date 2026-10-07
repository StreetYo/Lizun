interface Span {
  start: number;
  size: number;
}

/**
 * Drag to reorder a row or a column. The dragged item follows the pointer, the items it
 * passes step aside, and nothing moves in the DOM until the drop: the view reads `offset()`
 * for every item and `onDrop` saves the new order. Items should animate `translate` unless
 * `instant` is set.
 */
export class Reorder {
  /** Index of the item being dragged. */
  dragging = $state<number | null>(null);
  /** Index the dragged item lands on if it is dropped now. */
  target = $state(0);
  /** Set for a moment after a drop, while the items take their new places. */
  instant = $state(false);

  #delta = $state(0);
  /** After a drop, the dropped item starts where it was let go and slides into its place. */
  #landing = $state<{ index: number; offset: number } | null>(null);
  #spans: Span[] = [];
  #origin = 0;
  #pointerId = -1;
  readonly #axis: 'x' | 'y';
  readonly #gap: number;
  readonly #onDrop: (from: number, to: number) => void;

  /** `gap` is the space between two items. */
  constructor(axis: 'x' | 'y', gap: number, onDrop: (from: number, to: number) => void) {
    this.#axis = axis;
    this.#gap = gap;
    this.#onDrop = onDrop;
  }

  /** Starts a drag from a pointerdown; `items` are all items in their current order. */
  start(event: PointerEvent, index: number, items: HTMLElement[]): void {
    if (event.button !== 0 || this.dragging !== null) return;
    event.preventDefault();
    this.#spans = items.map((item) => {
      const rect = item.getBoundingClientRect();
      return this.#axis === 'x' ? { start: rect.left, size: rect.width } : { start: rect.top, size: rect.height };
    });
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
    this.#pointerId = event.pointerId;
    this.#origin = this.#position(event);
    this.#delta = 0;
    this.#landing = null;
    this.target = index;
    this.dragging = index;
  }

  move(event: PointerEvent): void {
    if (this.dragging === null || event.pointerId !== this.#pointerId) return;
    const spans = this.#spans;
    const dragged = spans[this.dragging];
    const first = spans[0];
    const last = spans[spans.length - 1];
    // The item cannot leave the row.
    const min = first.start - dragged.start;
    const max = last.start + last.size - dragged.start - dragged.size;
    this.#delta = Math.min(max, Math.max(min, this.#position(event) - this.#origin));
    // An item steps aside once the leading edge of the dragged one passes its middle.
    const start = dragged.start + this.#delta;
    const end = start + dragged.size;
    const from = this.dragging;
    const middle = (span: Span) => span.start + span.size / 2;
    const passedAfter = spans.filter((span, i) => i > from && middle(span) < end).length;
    const passedBefore = spans.filter((span, i) => i < from && middle(span) > start).length;
    this.target = from + passedAfter - passedBefore;
  }

  /** Ends the drag on pointerup (drops) or pointercancel (puts the item back). */
  end(event: PointerEvent): void {
    if (this.dragging === null || event.pointerId !== this.#pointerId) return;
    const from = this.dragging;
    const to = this.target;
    this.dragging = null;
    if (event.type !== 'pointerup' || from === to) return;

    const spans = this.#spans;
    const landsAt = from < to ? spans[to].start + spans[to].size - spans[from].size : spans[to].start;
    this.#landing = { index: to, offset: spans[from].start + this.#delta - landsAt };
    this.instant = true;
    this.#onDrop(from, to);
    // Two frames: the first one draws the new order without animation.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        this.instant = false;
        this.#landing = null;
      }),
    );
  }

  /** How far the item at `index` is moved from its place right now, in pixels. */
  offset(index: number): number {
    if (this.#landing?.index === index) return this.#landing.offset;
    const from = this.dragging;
    if (from === null) return 0;
    if (index === from) return this.#delta;
    const to = this.target;
    const stride = this.#spans[from].size + this.#gap;
    if (from < to && index > from && index <= to) return -stride;
    if (to < from && index >= to && index < from) return stride;
    return 0;
  }

  #position(event: PointerEvent): number {
    return this.#axis === 'x' ? event.clientX : event.clientY;
  }
}
