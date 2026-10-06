const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex="0"]';

export function trapTab(event: KeyboardEvent, container: HTMLElement) {
  if (event.key !== 'Tab') return;
  const items = [...container.querySelectorAll<HTMLElement>(FOCUSABLE)];
  const first = items[0];
  const last = items.at(-1);
  if (!first || !last) return;
  const edge = event.shiftKey ? first : last;
  const active = document.activeElement;
  if (active !== container && active !== edge && container.contains(active)) return;
  event.preventDefault();
  (event.shiftKey ? last : first).focus();
}

export const paletteReturn: { element: HTMLElement | null } = { element: null };

const OTHER_LAYERS = 'dialog[open], .notices';

export function containFocus(container: HTMLElement): () => void {
  const guard = (event: FocusEvent) => {
    const target = event.target as HTMLElement | null;
    if (!target || container.contains(target) || target.closest(OTHER_LAYERS)) return;
    container.focus();
  };
  document.addEventListener('focusin', guard);
  return () => document.removeEventListener('focusin', guard);
}
