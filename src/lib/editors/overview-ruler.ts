import { rulerMarks, scrollForRuler, thinMarks, thumbOf, type Band } from './chunk-bands';

export const RULER_WIDTH = 22;
const LANE_LEFT = 3, LANE_STEP = 6, LANE_WIDTH = 5;
const WORD = { add: 'added', rem: 'removed', chg: 'changed' } as const;

export type RulerSource = {
  scroller: HTMLElement;
  bands(): Band[];
  current(): number;
  total(): number;
  onJump(index: number): void;
};

/** A strip beside the editors in place of the scrollbar: one mark per change in three lanes, plus the visible window. */
export class OverviewRuler {
  readonly element = document.createElement('div');
  private readonly thumb = document.createElement('div');
  private readonly source: RulerSource;
  private readonly resize: ResizeObserver;
  private frame = 0;
  private grab: number | null = null;

  constructor(source: RulerSource) {
    this.source = source;
    this.element.className = 'cm-ruler';
    this.element.setAttribute('role', 'group');
    this.element.setAttribute('aria-label', 'Change overview');
    this.thumb.className = 'cm-ruler-thumb';
    this.element.append(this.thumb);
    this.element.addEventListener('pointerdown', this.press);
    this.element.addEventListener('pointermove', this.move);
    this.element.addEventListener('pointerup', this.release);
    this.element.addEventListener('pointercancel', this.release);
    source.scroller.addEventListener('scroll', this.place, { passive: true });
    this.resize = new ResizeObserver(() => this.schedule());
    this.resize.observe(this.element);
  }

  schedule() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => { this.frame = 0; this.render(); });
  }

  private render() {
    const { scroller, current, total } = this.source, count = total();
    const marks = thinMarks(rulerMarks(this.source.bands(), scroller.scrollHeight, this.element.clientHeight));
    const active = current();
    const buttons = marks.map(mark => {
      const button = document.createElement('button');
      button.type = 'button'; button.tabIndex = -1;
      button.className = `cm-ruler-mark cm-ruler-${mark.kind}${mark.index === active ? ' cm-ruler-cur' : ''}`;
      button.dataset.jump = String(mark.index);
      button.setAttribute('aria-label', `Change ${mark.index + 1} of ${count}, ${WORD[mark.kind]}`);
      button.title = button.getAttribute('aria-label')!;
      Object.assign(button.style, { top: `${mark.top}px`, height: `${mark.height}px`, left: `${LANE_LEFT + mark.lane * LANE_STEP}px`, width: `${LANE_WIDTH}px` });
      return button;
    });
    this.element.replaceChildren(...buttons, this.thumb);
    this.place();
  }

  private readonly place = () => {
    const { scroller } = this.source, height = this.element.clientHeight;
    const thumb = thumbOf(scroller.scrollTop, scroller.clientHeight, scroller.scrollHeight, height);
    this.thumb.style.height = `${thumb.height}px`;
    this.thumb.style.transform = `translateY(${thumb.top}px)`;
  };

  private scrollTo(y: number) {
    const { scroller } = this.source;
    scroller.scrollTop = scrollForRuler(y, this.grab ?? 0, scroller.clientHeight, scroller.scrollHeight, this.element.clientHeight);
  }

  private offset(event: PointerEvent) { return event.clientY - this.element.getBoundingClientRect().top; }

  private readonly press = (event: PointerEvent) => {
    if (event.button !== 0) return;
    const mark = (event.target as Element).closest<HTMLElement>('[data-jump]');
    if (mark) { this.source.onJump(Number(mark.dataset.jump)); return; }
    const { scroller } = this.source, height = this.element.clientHeight, y = this.offset(event);
    const thumb = thumbOf(scroller.scrollTop, scroller.clientHeight, scroller.scrollHeight, height);
    this.grab = y >= thumb.top && y <= thumb.top + thumb.height ? y - thumb.top : thumb.height / 2;
    this.element.setPointerCapture(event.pointerId);
    this.thumb.classList.add('cm-ruler-drag');
    this.scrollTo(y);
  };

  private readonly move = (event: PointerEvent) => { if (this.grab !== null) this.scrollTo(this.offset(event)); };

  private readonly release = () => { this.grab = null; this.thumb.classList.remove('cm-ruler-drag'); };

  destroy() {
    cancelAnimationFrame(this.frame);
    this.resize.disconnect();
    this.source.scroller.removeEventListener('scroll', this.place);
    this.element.remove();
  }
}
