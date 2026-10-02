import { motionMs } from './appearance';

/** Exit for modal dialogs; --fade lets ::backdrop fade in step with the box. */
export function dialogOut(node: HTMLElement, { duration = 120 } = {}) {
  return {
    duration: motionMs(duration),
    css: (t: number) => `opacity: ${t}; transform: translateY(${(1 - t) * 6}px) scale(${0.98 + 0.02 * t});`,
    tick: (t: number) => node.style.setProperty('--fade', String(t)),
  };
}
