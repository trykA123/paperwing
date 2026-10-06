import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';

export function deferred() {
    let resolve, reject;
    const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
    return { promise, resolve, reject };
}

export async function withIpc(handler, run) {
    const previousWindow = globalThis.window;
    globalThis.window = { crypto: globalThis.crypto };
    mockIPC(handler);
    try { await run(); }
    finally {
        clearMocks();
        if (previousWindow === undefined) delete globalThis.window;
        else globalThis.window = previousWindow;
    }
}
