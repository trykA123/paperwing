import { readFileSync } from 'node:fs';
import { api } from '../api';

const source = readFileSync(new URL('../../App.svelte', import.meta.url), 'utf8').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1];
const script = new Bun.Transpiler({ loader: 'ts' }).transformSync(source).replace(/^import .*;\s*$/gm, '');

export function appLifecycle(state) {
    let mount, close;
    const effects = [], timers = [], destroyed = [];
    const effect = run => effects.push(run);
    effect.pre = () => {};
    const window = { addEventListener() {}, removeEventListener() {} };
    const currentWindow = {
        onCloseRequested: async run => { close = run; return () => {}; },
        destroy: async () => { destroyed.push(true); },
    };
    const bindings = {
        app: state, api, onMount: run => { mount = run; },
        $state: Object.assign(value => value, { snapshot: value => structuredClone(value) }),
        $derived: value => value, $effect: effect, isTauri: () => true, getCurrentWindow: () => currentWindow,
        matchMedia: () => ({ matches: false, ...window }), window,
        setTimeout: (run, ms) => { const timer = { run, ms }; timers.push(timer); return timer; },
        clearTimeout: timer => { const index = timers.indexOf(timer); if (index >= 0) timers.splice(index, 1); },
        applyAppearance() {}, onSystemThemeChange() {}, benchmarkEnabled: false,
        compareFullscreen: { active: false, follow() {}, toggle: async () => {}, back: async () => {} }, escapeLeaves: () => false,
    };
    new Function(...Object.keys(bindings), script)(...Object.values(bindings));
    return {
        effects, timers, destroyed,
        mount: () => mount(),
        close: () => close({ preventDefault() {} }),
        async autosave() {
            const cleanups = effects.map(run => run());
            try { await Promise.all(timers.filter(timer => timer.ms === 400).map(timer => timer.run())); }
            finally { for (const cleanup of cleanups) cleanup?.(); }
        },
    };
}
