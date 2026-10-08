import type { Band } from './chunk-bands';

const round = (value: number) => Math.round(value * 10) / 10;

/** The outline of one change between the left and right editors; a wedge side narrows to a point. */
export function ribbonPath(band: Band, width: number): string {
  const middle = round((band.top + band.bottom) / 2), curve = round(width / 2);
  const [leftTop, leftBottom] = band.wedgeA ? [middle, middle] : [round(band.top), round(band.bottom)];
  const [rightTop, rightBottom] = band.wedgeB ? [middle, middle] : [round(band.top), round(band.bottom)];
  return `M0 ${leftTop}C${curve} ${leftTop} ${curve} ${rightTop} ${width} ${rightTop}L${width} ${rightBottom}C${curve} ${rightBottom} ${curve} ${leftBottom} 0 ${leftBottom}Z`;
}

export const ribbonLabelY = (band: Band): number => round((band.top + band.bottom) / 2);
