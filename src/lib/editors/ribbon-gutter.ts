import type { Band } from './chunk-bands';
import { ribbonLabelY, ribbonPath } from './ribbon-path';

export const RIBBON_WIDTH = 56;
const SVG = 'http://www.w3.org/2000/svg';
const WORD = { add: 'added', rem: 'removed', chg: 'changed' } as const;

export type RibbonSource = {
  bands(): Band[];
  current(): number;
  total(): number;
  onJump(index: number): void;
};

/** The middle column between the two editors: one ribbon per change, the current one stronger and numbered. */
export class RibbonGutter {
  readonly element = document.createElement('div');
  private readonly svg = document.createElementNS(SVG, 'svg');
  private frame = 0;
  private readonly source: RibbonSource;

  constructor(source: RibbonSource) {
    this.source = source;
    this.element.className = 'cm-ribbons';
    this.svg.setAttribute('aria-hidden', 'true');
    this.element.append(this.svg);
    this.svg.addEventListener('click', event => {
      const target = (event.target as Element).closest<SVGElement>('[data-chunk]');
      if (target) source.onJump(Number(target.dataset.chunk));
    });
  }

  schedule() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => { this.frame = 0; this.draw(); });
  }

  draw() {
    const width = this.element.clientWidth || RIBBON_WIDTH, current = this.source.current(), total = this.source.total();
    let shapes = '', top = '';
    for (const band of this.source.bands()) {
      const active = band.index === current;
      const shape = `<g class="cm-ribbon cm-ribbon-${band.kind}${active ? ' cm-ribbon-cur' : ''}"><path data-chunk="${band.index}" d="${ribbonPath(band, width)}"><title>Change ${band.index + 1} of ${total}, ${WORD[band.kind]}</title></path>`;
      const label = active ? `<text x="${width / 2}" y="${ribbonLabelY(band) + 4}" text-anchor="middle">${band.index + 1}</text>` : '';
      if (active) top = `${shape}${label}</g>`; else shapes += `${shape}</g>`;
    }
    this.svg.innerHTML = shapes + top;
  }

  destroy() {
    cancelAnimationFrame(this.frame);
    this.element.remove();
  }
}
