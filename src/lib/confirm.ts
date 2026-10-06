import { writable } from 'svelte/store';

export type ConfirmOptions = {
  title?: string;
  kind?: 'info' | 'warning' | 'error';
  okLabel?: string;
  cancelLabel?: string;
  /** Styles the confirm button as a destructive action and focuses Cancel first. */
  destructive?: boolean;
  /** Optional extra choice shown under the message, unchecked by default. */
  check?: { label: string; hint?: string; disabled?: boolean; okLabel?: string };
};
export type ConfirmResult = { accepted: boolean; checked: boolean };
export type ConfirmRequest = ConfirmOptions & { message: string; resolve: (result: ConfirmResult) => void };

export const confirmQueue = writable<ConfirmRequest[]>([]);

/** Like `confirm`, but also reports the optional checkbox. */
export function confirmWith(message: string, options: ConfirmOptions = {}): Promise<ConfirmResult> {
  return new Promise(resolve => { confirmQueue.update(queue => [...queue, { ...options, message, resolve }]); });
}

/** In-app replacement for the native confirm dialog; same call shape as the Tauri dialog plugin. */
export function confirm(message: string, options: ConfirmOptions = {}): Promise<boolean> {
  return confirmWith(message, options).then(result => result.accepted);
}

export const ask = confirm;

export function answer(accepted: boolean, checked = false) {
  confirmQueue.update(queue => {
    queue[0]?.resolve({ accepted, checked: accepted && checked });
    return queue.slice(1);
  });
}
