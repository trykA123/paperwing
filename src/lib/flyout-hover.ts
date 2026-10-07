export type Point = { x: number; y: number };
export type Rect = { left: number; top: number; right: number; bottom: number };
export type Timers = { set: (run: () => void, ms: number) => unknown; clear: (id: unknown) => void };

export const HOVER_DELAY = 150;
export const TYPEAHEAD_RESET = 600;

const side = (p: Point, a: Point, b: Point) => (p.x - b.x) * (a.y - b.y) - (a.x - b.x) * (p.y - b.y);

export function inTriangle(point: Point, a: Point, b: Point, c: Point): boolean {
  const signs = [side(point, a, b), side(point, b, c), side(point, c, a)];
  return !(signs.some(sign => sign < 0) && signs.some(sign => sign > 0));
}

/** The pointer is on its way from where it left the trigger to the flyout: inside the triangle from that point to the flyout's near edge. */
export function onSafePath(pointer: Point, origin: Point, flyout: Rect): boolean {
  const toLeft = flyout.left >= origin.x;
  const edge = toLeft ? flyout.left : flyout.right;
  return inTriangle(pointer, origin, { x: edge, y: flyout.top }, { x: edge, y: flyout.bottom });
}

/** Opens a menu after the pointer rests on its trigger and closes it once the pointer leaves, unless it keeps moving toward the menu. */
export class HoverIntent {
  #opening: unknown;
  #closing: unknown;
  #origin: Point | null = null;
  #target: string | null = null;

  constructor(private readonly timers: Timers, private readonly open: (id: string) => void, private readonly close: () => void, private readonly delay = HOVER_DELAY) {}

  get closing() { return this.#closing !== undefined; }

  enterTrigger(id: string, current: string | null) {
    this.#cancelClose();
    if (current === id) { this.#cancelOpen(); return; }
    if (this.#target === id && this.#opening !== undefined) return;
    this.#cancelOpen();
    this.#target = id;
    this.#opening = this.timers.set(() => { this.#opening = undefined; this.#target = null; this.open(id); }, this.delay);
  }

  enterPanel() {
    this.#cancelOpen();
    this.#cancelClose();
  }

  leave(pointer: Point, isOpen: boolean) {
    this.#cancelOpen();
    if (!isOpen) return;
    this.#origin = pointer;
    this.#scheduleClose();
  }

  /** While a close is pending, a pointer still on the safe path keeps the menu open. */
  move(pointer: Point, flyout: Rect) {
    if (this.#closing === undefined || !this.#origin) return;
    if (!onSafePath(pointer, this.#origin, flyout)) return;
    this.#origin = pointer;
    this.#scheduleClose();
  }

  cancel() {
    this.#cancelOpen();
    this.#cancelClose();
  }

  #scheduleClose() {
    if (this.#closing !== undefined) this.timers.clear(this.#closing);
    this.#closing = this.timers.set(() => { this.#closing = undefined; this.close(); }, this.delay);
  }

  #cancelOpen() {
    if (this.#opening !== undefined) this.timers.clear(this.#opening);
    this.#opening = undefined;
    this.#target = null;
  }

  #cancelClose() {
    if (this.#closing !== undefined) this.timers.clear(this.#closing);
    this.#closing = undefined;
  }
}

/** Index of the next label that starts with the typed text; one letter, or a repeated one, moves past the focused item. */
export function typeaheadMatch(labels: readonly string[], current: number, typed: string): number {
  const text = typed.toLowerCase();
  if (!text) return -1;
  const repeated = [...text].every(char => char === text[0]);
  const query = repeated ? text[0] : text;
  const from = repeated ? current + 1 : current;
  for (let step = 0; step < labels.length; step++) {
    const at = (from + step + labels.length) % labels.length;
    if (labels[at].toLowerCase().startsWith(query)) return at;
  }
  return -1;
}

/** Next index in a wrapping list, for arrows, Home and End. */
export function menuStep(key: string, current: number, size: number): number | undefined {
  if (!size) return undefined;
  if (key === 'ArrowDown') return (current + 1) % size;
  if (key === 'ArrowUp') return (current - 1 + size) % size;
  if (key === 'Home') return 0;
  if (key === 'End') return size - 1;
  return undefined;
}
