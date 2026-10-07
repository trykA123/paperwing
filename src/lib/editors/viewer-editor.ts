import type { CompareEditor, CompareEditorInit, EditorChange, EditorEvent, EditorSettings, Side, SideSnapshot } from '../editor';
import { contentBytes, type SideContent, type TextFormat } from '../text-format';
import { diffLines, type LineHunk } from './line-diff';
import { Listeners, stepIndex } from './shared';
import { buildRows, ROW_ADDED, ROW_CHANGED, ROW_GAP, ROW_REMOVED, type Rows } from './viewer-rows';

const ROW_HEIGHT = 20, OVERSCAN = 10, CLIP = 4000, NUMBER_PX = 56, PAD_PX = 16, BAR_PX = 14;
const trimmed = (line: string) => line.trim();

function element(tag: string, className: string, text = ''): HTMLElement {
  const node = document.createElement(tag);
  node.className = className;
  if (text) node.textContent = text;
  return node;
}

function textCell(text: string): HTMLElement {
  const cell = element('span', 'vw-tx');
  cell.append(element('span', 'vw-text', text));
  return cell;
}

const clip = (line: string) => (line.length > CLIP ? `${line.slice(0, CLIP)} [line truncated]` : line);

class ViewerEditor implements CompareEditor {
  readonly kind = 'viewer' as const;
  readonly capabilities = { edit: false, search: false, highlight: false };
  private contents: Record<Side, SideContent>;
  private lines: Record<Side, string[]> = { left: [], right: [] };
  private settings: EditorSettings;
  private hunks: readonly LineHunk[] = [];
  private rows!: Rows;
  private changeList: EditorChange[] = [];
  private current = -1;
  private maxChars = 0;
  private charWidth = 8;
  private firstRendered = -1;
  private readonly listeners = new Listeners();
  private readonly root = element('div', 'vw');
  private readonly scroller = element('div', 'vw-scroll');
  private readonly spacer = element('div', 'vw-spacer');
  private readonly bar = element('div', 'vw-hbar');
  private readonly barSpacer = element('div', 'vw-hspacer');
  private readonly layer = element('div', 'vw-layer');
  private readonly probe = element('span', 'vw-probe', 'M'.repeat(50));
  private readonly resize: ResizeObserver | null;

  constructor(private readonly init: CompareEditorInit) {
    this.contents = { left: init.left, right: init.right };
    this.settings = init.settings;
    this.scroller.tabIndex = 0;
    this.scroller.setAttribute('role', 'region');
    this.scroller.setAttribute('aria-label', 'Large file comparison, read-only');
    this.scroller.append(this.spacer, this.layer);
    this.bar.append(this.barSpacer);
    this.root.append(this.scroller, this.bar, this.probe);
    init.host.append(this.root);
    this.scroller.addEventListener('scroll', this.render);
    this.scroller.addEventListener('wheel', this.wheel, { passive: false });
    this.scroller.addEventListener('keydown', this.key);
    this.bar.addEventListener('scroll', this.shift);
    this.resize = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => this.layout());
    this.resize?.observe(this.scroller);
    this.rebuild();
  }

  private rebuild() {
    this.lines = { left: this.contents.left.format.text.split('\n'), right: this.contents.right.format.text.split('\n') };
    this.hunks = diffLines(this.lines.left, this.lines.right, this.settings.ignoreWhitespace ? trimmed : undefined);
    this.changeList = this.hunks.map(([a0, a1, b0, b1]) => ({ left: { start: a0, count: a1 - a0 }, right: { start: b0, count: b1 - b0 } }));
    this.maxChars = Math.min(CLIP + 20, Math.max(0, ...[this.lines.left, this.lines.right].map(lines => lines.reduce((max, line) => Math.max(max, line.length), 0))));
    this.current = Math.min(this.current, this.changeList.length - 1);
    this.buildRows();
    this.listeners.emit({ type: 'changes' });
  }

  private buildRows() {
    this.rows = buildRows({ leftLines: this.lines.left.length, rightLines: this.lines.right.length, hunks: this.hunks, layout: this.settings.layout, hideUnchanged: this.settings.hideUnchanged });
    this.measure();
  }

  private measure() {
    const width = this.probe.getBoundingClientRect().width / 50;
    if (width > 0) this.charWidth = width;
    const inline = this.settings.layout === 'inline';
    const paneText = (this.scroller.clientWidth - 2 * NUMBER_PX) / (inline ? 1 : 2) - PAD_PX;
    const overflow = Math.max(0, Math.ceil(this.maxChars * this.charWidth - paneText));
    this.bar.hidden = overflow < 1;
    this.root.style.setProperty('--vw-hbar', overflow < 1 ? '0px' : `${BAR_PX}px`);
    this.barSpacer.style.width = `${this.bar.clientWidth + overflow}px`;
    this.spacer.style.height = `${this.rows.count * ROW_HEIGHT}px`;
    this.firstRendered = -1;
    this.shift();
    this.render();
  }

  private readonly shift = () => { this.layer.style.setProperty('--vw-shift', `${-this.bar.scrollLeft}px`); };

  private readonly wheel = (event: WheelEvent) => {
    if (this.bar.hidden || !(event.deltaX || event.shiftKey)) return;
    event.preventDefault();
    this.bar.scrollLeft += event.deltaX || event.deltaY;
  };

  private readonly key = (event: KeyboardEvent) => {
    if (this.bar.hidden || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return;
    event.preventDefault();
    this.bar.scrollLeft += event.key === 'ArrowLeft' ? -48 : 48;
  };

  private cell(className: string, number: number, text: string): HTMLElement {
    const cell = element('div', `vw-cell ${className}`.trim());
    cell.append(element('span', 'vw-no', number >= 0 ? String(number + 1) : ''), textCell(text));
    return cell;
  }

  private row(index: number): HTMLElement {
    const { kind, left, right } = this.rows, k = kind[index]!, a = left[index]!, b = right[index]!;
    const row = element('div', 'vw-row');
    if (k === ROW_GAP) { row.append(element('div', 'vw-gap', `${a} unchanged lines`)); return row; }
    const removed = k === ROW_REMOVED || k === ROW_CHANGED, added = k === ROW_ADDED || k === ROW_CHANGED;
    if (this.settings.layout === 'inline') {
      const number = element('span', 'vw-no', a >= 0 ? String(a + 1) : ''), number2 = element('span', 'vw-no', b >= 0 ? String(b + 1) : '');
      const line = a >= 0 ? this.lines.left[a]! : this.lines.right[b]!;
      const cell = element('div', `vw-cell ${removed ? 'is-removed' : added ? 'is-added' : ''}`.trim());
      cell.append(number, number2, textCell(clip(line)));
      row.append(cell);
      return row;
    }
    row.append(
      this.cell(removed ? 'is-removed' : added && k !== ROW_CHANGED ? 'is-void' : '', a, a >= 0 ? clip(this.lines.left[a]!) : ''),
      this.cell(added ? 'is-added' : removed ? 'is-void' : '', b, b >= 0 ? clip(this.lines.right[b]!) : ''),
    );
    return row;
  }

  private readonly render = () => {
    const height = this.scroller.clientHeight || 0;
    const start = Math.max(0, Math.floor(this.scroller.scrollTop / ROW_HEIGHT) - OVERSCAN);
    const end = Math.min(this.rows.count, Math.ceil((this.scroller.scrollTop + height) / ROW_HEIGHT) + OVERSCAN);
    if (start === this.firstRendered && this.layer.childElementCount === end - start) return;
    this.firstRendered = start;
    const fragment = document.createDocumentFragment();
    for (let index = start; index < end; index++) fragment.append(this.row(index));
    this.layer.replaceChildren(fragment);
    this.layer.style.transform = `translateY(${start * ROW_HEIGHT}px)`;
  };

  async configure(patch: Partial<EditorSettings>): Promise<void> {
    const previous = this.settings;
    this.settings = { ...previous, ...patch, readOnly: { left: true, right: true }, locked: true };
    if (this.settings.layout !== previous.layout || this.settings.hideUnchanged !== previous.hideUnchanged) this.buildRows();
    if (this.settings.ignoreWhitespace !== previous.ignoreWhitespace) this.rebuild();
  }

  setContent(side: Side, content: SideContent) {
    this.contents[side] = content;
    this.rebuild();
  }

  getText(side: Side): string { return this.contents[side].format.text; }
  getBytes(side: Side): number[] { return contentBytes(this.contents[side], this.getText(side)); }
  format(side: Side): TextFormat { return this.contents[side].format; }
  isReadOnly(): boolean { return true; }
  isDirty(): boolean { return false; }
  snapshot(side: Side): SideSnapshot { return { bytes: this.getBytes(side), token: null }; }
  markSaved() {}
  revert() {}
  changes(): readonly EditorChange[] { return this.changeList; }
  currentChange(): number { return this.current; }

  goToChange(direction: 1 | -1): number {
    this.current = stepIndex(this.current, direction, this.changeList.length);
    const row = this.rows.changeRows[this.current];
    if (row !== undefined) {
      this.scroller.scrollTop = Math.max(0, row * ROW_HEIGHT - this.scroller.clientHeight / 3);
      this.render();
    }
    return this.current;
  }

  copyChange(): boolean { return false; }
  undo(): boolean { return false; }
  redo(): boolean { return false; }
  openSearch(): boolean { return false; }
  focus() { this.scroller.focus(); }
  layout() { this.measure(); }
  on(listener: (event: EditorEvent) => void) { return this.listeners.add(listener); }

  dispose() {
    this.resize?.disconnect();
    this.scroller.removeEventListener('scroll', this.render);
    this.scroller.removeEventListener('wheel', this.wheel);
    this.scroller.removeEventListener('keydown', this.key);
    this.bar.removeEventListener('scroll', this.shift);
    this.root.remove();
    this.listeners.clear();
  }
}

export function createViewerEditor(init: CompareEditorInit): CompareEditor {
  return new ViewerEditor(init);
}
