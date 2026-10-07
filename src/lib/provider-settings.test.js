import { expect, test } from 'bun:test';
import { render } from 'svelte/server';
import { readFileSync } from 'node:fs';

await import('./test-support/svelte-loader.js');
const previousWindow = globalThis.window;
globalThis.window = { ...previousWindow, matchMedia: () => ({ matches: false }) };
const { default: Settings } = await import('../components/Settings.svelte');
const { app } = await import('./state.svelte');
if (previousWindow === undefined) delete globalThis.window;
else globalThis.window = previousWindow;

test('each source exposes an Enabled switch with the current checked state', () => {
    const previous = app.sources;
    app.sources = [true, false].map((enabled, index) => ({
        id: `admin-${index}`, name: 'admin', kind: 'manual', host: '', orgs: [], urls: [], enabled,
    }));
    try {
        const html = render(Settings).body;
        const switches = [...html.matchAll(/<label\b[^>]*>([\s\S]*?<input\b[^>]*role="switch"[^>]*>[\s\S]*?)<\/label>/g)];
        expect(switches).toHaveLength(2);
        switches.forEach(([label, content], index) => {
            expect(content).toContain('Enabled');
            expect(content).not.toContain('aria-checked');
            expect(content.includes(' checked')).toBe(index === 0);
            expect(label).toContain('aria-label="admin enabled"');
        });
    } finally { app.sources = previous; }
});

test('the source switch keeps its saved state when settings persistence fails', async () => {
    const component = readFileSync(new URL('../components/Settings.svelte', import.meta.url), 'utf8');
    const toggle = component.match(/  async function toggleProvider\([\s\S]*?(?=\n  async function remove)/)[0];
    const script = new Bun.Transpiler({ loader: 'ts' }).transformSync(toggle);
    const source = { id: 'admin', enabled: true };
    const notices = [];
    const state = {
        sources: [source], ws: {},
        saveSettings: async () => { throw Error('disk unavailable'); },
        toast: message => notices.push(message),
    };
    const run = new Function('app', '$state', `let busy = ''; ${script}; return toggleProvider;`)(
        state, { snapshot: structuredClone },
    );
    let prevented = false;
    await run(source, { preventDefault: () => { prevented = true; } });
    expect(prevented).toBe(true);
    expect(state.sources[0].enabled).toBe(true);
    expect(notices).toEqual(['Error: disk unavailable']);
});

test('the crate root exposes kernel without shadowing the Rust core crate', () => {
    const source = readFileSync(new URL('../../src-tauri/src/lib.rs', import.meta.url), 'utf8');
    expect(source).toContain('pub mod kernel;');
    expect(source).not.toMatch(/\bmod core;/);
});

test('architecture documents that disabling the only host source also blocks ls-remote', () => {
    const architecture = readFileSync(new URL('../../docs/architecture.md', import.meta.url), 'utf8');
    expect(architecture).toMatch(/github\.com[\s\S]*ls-remote[\s\S]*no traffic/);
});
