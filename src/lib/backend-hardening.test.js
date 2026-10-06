import './test-support/svelte-loader.js';
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { withIpc } from './test-support/ipc-fixture.js';
import { api } from './api';

const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');

test('draft Test and Load orgs carry a typed token without saving it', async () => {
    const source = { id: 'admin', name: 'admin', kind: 'ghe', host: 'new.invalid', orgs: [], urls: [] };
    const calls = [];
    await withIpc((command, args) => { calls.push({ command, args }); return command === 'test_source' ? 'admin' : []; }, async () => {
        await api.testSource(source, 'synthetic-typed-token');
        await api.listUserOrgs(source, 'synthetic-typed-token');
    });
    expect(calls).toEqual([
        { command: 'test_source', args: { source, token: 'synthetic-typed-token' } },
        { command: 'list_user_orgs', args: { source, token: 'synthetic-typed-token' } },
    ]);
    expect(read('../components/Settings.svelte')).not.toContain('await saveToken(); await task();');
});

test('disk IPC commands are async and the dead probe is removed', () => {
    const settings = read('../../src-tauri/src/settings.rs');
    const clone = read('../../src-tauri/src/clone.rs');
    const lib = read('../../src-tauri/src/lib.rs');
    expect(settings).toMatch(/pub async fn load_settings/);
    expect(settings).toMatch(/pub async fn save_settings/);
    expect(clone).not.toMatch(/pub fn start_clone/);
    expect(lib).toMatch(/async fn open_in_vscode/);
    expect(lib).not.toContain('paths_exist');
    expect(read('./api.ts')).not.toContain('pathsExist');
});

test('settings reads do not reconfigure sources', () => {
    const source = read('../../src-tauri/src/settings.rs');
    const body = source.slice(source.indexOf('pub fn load_settings'), source.indexOf('fn save_settings'));
    expect(body).not.toContain('configure_sources');
});

test('locale-sensitive decisions do not match stderr prose', () => {
    expect(read('../../src-tauri/src/commit.rs')).not.toContain('output.stderr_contains');
    expect(read('../../src-tauri/src/stash/ops.rs')).not.toContain('Try without --index');
    expect(read('./state.svelte.ts')).not.toContain("text.includes('not fully merged')");
});

test('object validation is shared by the four IPC boundaries', () => {
    for (const file of ['stash.rs', 'tags.rs', 'branch_cleanup/resolve.rs', 'github/pulls/repository.rs']) {
        expect(read(`../../src-tauri/src/${file}`)).toContain('crate::object_id::valid');
    }
});

test('get_commits stops publishing unused cache entries', () => {
    expect(read('../../src-tauri/src/github.rs')).not.toContain('commit_cache::remember');
});

const { app } = await import('./state.svelte.ts');
const { confirmQueue, answer } = await import('./confirm');
const { get } = await import('svelte/store');
const { linuxPlatform } = await import('./test-support/platform-fixture.js');

test('German notMerged errors still offer force deletion', async () => {
    const calls = [];
    await withIpc((command, args) => {
        if (command === 'delete_branch') {
            calls.push(args.force);
            if (!args.force) throw { kind: 'notMerged', message: 'Der Branch ist nicht vollständig zusammengeführt.' };
            return { sha: 'abcd' };
        }
        if (command === 'local_status') return [];
        throw Error(command);
    }, async () => {
        const state = new app.constructor();
        const pending = state.deleteLocalBranch('/fixture', 'admin', 'topic');
        answer(true);
        try {
            for (let step = 0; step < 12; step++) await Promise.resolve();
            expect(get(confirmQueue)[0]?.title).toBe('Unmerged branch');
            answer(true);
            await pending;
            expect(calls).toEqual([false, true]);
        } finally { answer(false); await pending; }
    });
});

test('settings initialization shows the backup restoration toast', async () => {
    await withIpc(command => {
        if (command === 'load_settings') return { sources: [], workspace: null, restoredFromBackup: true };
        if (command === 'platform_info') return linuxPlatform;
        if (command === 'plugin:event|listen') return 1;
        if (command === 'activity_snapshot') return [];
        throw Error(command);
    }, async () => {
        const state = new app.constructor();
        const notices = [];
        state.toast = message => notices.push(message);
        await state.init();
        expect(state.ready).toBe(true);
        expect(notices).toContain('Settings were restored from a backup');
    });
});

test('resolving the settings read path does not create files or directories', () => {
    const source = read('../../src-tauri/src/settings.rs');
    const start = source.indexOf('fn settings_file');
    const body = source.slice(start, source.indexOf('\n}', start) + 2);
    expect(body).not.toContain('create_dir_all');
    expect(body).not.toContain('std::fs::copy');
});
