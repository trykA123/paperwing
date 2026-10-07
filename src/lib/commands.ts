import type { IconName, IconTone } from '../components/Icon.svelte';
import type { SetItem } from './api';
import { app } from './state.svelte';
import { historyDrawer } from './history-drawer.svelte';
import { paletteReturn } from './focus-trap';
import { railShortcut } from './rail';
import { plural } from './plural';

export type Command = { id: string; label: string; icon: IconName; tone?: IconTone; enabled: boolean; reason?: string | null; run: () => void | Promise<void> };

export const GROUPS = ['Actions', 'Files', 'Navigate', 'View'] as const;
export type CommandGroup = (typeof GROUPS)[number];

const GROUP_OF: Record<string, CommandGroup> = {
  'tab-next': 'Navigate', 'tab-previous': 'Navigate', 'tab-close': 'Navigate', set: 'Navigate', 'search-code': 'Navigate', 'new-set': 'Navigate', add: 'Navigate', settings: 'Navigate',
  sidebar: 'View', details: 'View', theme: 'View',
  'copy-left': 'Files', 'copy-right': 'Files', recovery: 'Files', 'editor-save': 'Files', 'editor-save-left': 'Files', 'editor-save-right': 'Files', 'difference-next': 'Files',
  'difference-previous': 'Files', 'hunk-left': 'Files', 'hunk-right': 'Files', 'file-undo': 'Files',
};

const SHORTCUT_OF: Record<string, string> = {
  'tab-next': 'Ctrl Tab', 'tab-previous': 'Ctrl Shift Tab', 'tab-close': 'Ctrl W', 'editor-save': 'Ctrl S', 'search-code': 'Ctrl Shift F',
  'difference-next': 'F7', 'difference-previous': 'Shift F7', 'hunk-left': 'Ctrl Alt ←', 'hunk-right': 'Ctrl Alt →',
};

export const commandGroup = (command: Pick<Command, 'id'>): CommandGroup => (command.id.startsWith('set:') ? 'Navigate' : GROUP_OF[command.id] ?? 'Actions');
export const commandShortcut = (command: Pick<Command, 'id'>): string | undefined => SHORTCUT_OF[command.id];
export { isRisky } from './palette';

export function commands(items: SetItem[] = app.actionItems): Command[] {
  const local = items.filter(item => app.local[app.dest(item)]?.repo);
  const managed = local.filter(item => !item.path);
  const behind = managed.filter(item => { const l = app.local[app.dest(item)]; return (l?.behind ?? 0) > 0 && !(l!.ahead > 0 && !!l!.branch); });
  const offRef = managed.filter(item => !app.onRef(item));
  const pushable = local.filter(item => { const l = app.local[app.dest(item)]; return !!l?.branch && (l.ahead > 0 || !l.upstream); });
  const idle = !app.running && !app.clonePreparing && !app.gitBusy && app.ready;
  const rootIdle = idle && app.rootSupport.valid;
  const folderIdle = items.length > 0 && items.every(item => item.path) ? idle : rootIdle;
  const recovery = app.capability('recovery');
  const editor = app.editorActions[app.activeTabId];
  const copy = app.copyActions[app.activeTabId];
  return [
    { id: 'tab-next', label: 'Next tab', icon: 'chevron', enabled: app.tabs.length > 1, run: () => app.cycleTab(1) },
    { id: 'tab-previous', label: 'Previous tab', icon: 'chevron', enabled: app.tabs.length > 1, run: () => app.cycleTab(-1) },
    { id: 'tab-close', label: 'Close current tab', icon: 'close', enabled: !!app.activeTab, run: () => app.closeTab(app.activeTabId) },
    { id: 'copy-left', label: 'Copy selected file or folder to left', icon: 'copy', reason: copy?.leftReason ?? app.fileCapability('copy').reason, enabled: idle && app.fileCapability('copy').supported && !!copy?.left && !app.copyRequest, run: () => copy?.copy('left') },
    { id: 'copy-right', label: 'Copy selected file or folder to right', icon: 'copy', reason: copy?.rightReason ?? app.fileCapability('copy').reason, enabled: idle && app.fileCapability('copy').supported && !!copy?.right && !app.copyRequest, run: () => copy?.copy('right') },
    { id: 'recovery', label: 'Open filesystem recovery', icon: 'refresh', reason: recovery.reason, enabled: app.ready && recovery.supported && !app.copyRequest, run: () => { if (app.capability('recovery').supported) app.recoveryOpen = true; } },
    { id: 'editor-save', label: 'Save changed files', icon: 'check', reason: editor?.saveReasons?.find(Boolean) ?? app.fileCapability('edit').reason, enabled: app.fileCapability('edit').supported && !!editor?.canSave, run: () => editor?.save() },
    { id: 'editor-save-left', label: 'Save left file', icon: 'check', reason: editor?.saveReasons?.[0] ?? app.fileCapability('edit').reason, enabled: app.fileCapability('edit').supported && !!editor?.canSaveLeft, run: () => editor?.save(0) },
    { id: 'editor-save-right', label: 'Save right file', icon: 'check', reason: editor?.saveReasons?.[1] ?? app.fileCapability('edit').reason, enabled: app.fileCapability('edit').supported && !!editor?.canSaveRight, run: () => editor?.save(1) },
    { id: 'difference-next', label: 'Next difference', icon: 'chevron', enabled: !!editor?.canNavigate, run: () => editor?.next(1) },
    { id: 'difference-previous', label: 'Previous difference', icon: 'chevron', enabled: !!editor?.canNavigate, run: () => editor?.next(-1) },
    { id: 'hunk-left', label: 'Copy current hunk to left', icon: 'copy', reason: editor?.saveReasons?.[0] ?? app.fileCapability('edit').reason, enabled: app.fileCapability('edit').supported && !!editor?.canCopyLeft, run: () => editor?.copy('left') },
    { id: 'hunk-right', label: 'Copy current hunk to right', icon: 'copy', reason: editor?.saveReasons?.[1] ?? app.fileCapability('edit').reason, enabled: app.fileCapability('edit').supported && !!editor?.canCopyRight, run: () => editor?.copy('right') },
    { id: 'file-undo', label: 'Undo saved filesystem operation', icon: 'refresh', reason: editor?.undoReason ?? recovery.reason, enabled: recovery.supported && !!editor?.canUndo, run: () => editor?.undo() },
    { id: 'set', label: `Open ${app.set.name}`, icon: 'folder', enabled: app.ready, run: () => app.openView({ kind: 'set' }) },
    ...app.ws.sets.filter(set => set.id !== app.set.id).map(set => ({
      id: `set:${set.id}`, label: `Open set: ${set.name}`, icon: 'folder' as const, enabled: app.ready,
      run: () => app.openView({ kind: 'set' }, set.id),
    })),
    { id: 'search-code', label: 'Search code across set', icon: 'search', reason: 'Clone at least one repository of the set first', enabled: app.ready && app.set.items.some(item => app.local[app.dest(item)]?.repo), run: () => app.openCodeSearch() },
    { id: 'new-set', label: 'New set', icon: 'plus', enabled: app.ready, run: () => app.newSet() },
    { id: 'add', label: 'Browse repositories', icon: 'search', enabled: app.ready, run: () => app.goAddRepos() },
    { id: 'settings', label: 'Settings', icon: 'gear', enabled: app.ready, run: () => app.openView({ kind: 'settings' }) },
    { id: 'sidebar', label: `${app.ws.shell.sidebarVisible ? 'Hide' : 'Show'} sidebar`, icon: 'panel', enabled: true,
      run: () => { app.ws.shell.sidebarVisible = !app.ws.shell.sidebarVisible; } },
    { id: 'details', label: `${app.ws.shell.rightVisible ? 'Hide' : 'Show'} details`, icon: 'panel', enabled: app.view.kind !== 'settings' && app.view.kind !== 'codeSearch',
      run: () => { app.ws.shell.rightVisible = !app.ws.shell.rightVisible; } },
    { id: 'theme', label: `Use ${app.ws.theme === 'dark' ? 'light' : 'dark'} theme`, icon: 'theme', enabled: true,
      run: () => { app.ws.theme = app.ws.theme === 'dark' ? 'light' : 'dark'; } },
    { id: 'status', label: 'Refresh local status', icon: 'refresh', tone: 'sync' as const, reason: app.rootSupport.reason, enabled: folderIdle && !!items.length,
      run: async () => { await app.checkExists(items.map(item => app.dest(item))); } },
    { id: 'compare', label: 'Compare repository refs', icon: 'copy', tone: 'inspect' as const, reason: app.rootSupport.reason, enabled: rootIdle && !!items.length && !items[0]?.path,
      run: () => app.openCompare(items[0]) },
    { id: 'set-compare', label: 'Compare every repository across set', icon: 'copy', tone: 'inspect' as const, reason: app.rootSupport.reason, enabled: rootIdle && !!app.set.items.length && !app.set.items.some(item => item.path), run: () => app.openSetCompare() },
    { id: 'clone', label: `Clone ${plural(items.length, 'repository', 'repositories')}`, icon: 'folder', tone: 'sync' as const, reason: app.rootSupport.reason, enabled: rootIdle && !!items.length && !items.some(item => item.path),
      run: () => app.startClone(items) },
    { id: 'fetch', label: `Fetch ${plural(managed.length, 'repository', 'repositories')}`, icon: 'refresh', tone: 'sync' as const, reason: app.rootSupport.reason, enabled: rootIdle && !!managed.length,
      run: () => app.startClone(managed, 'fetch') },
    { id: 'pull', label: `Pull ${plural(behind.length, 'repository', 'repositories')} (fast-forward)`, icon: 'download', tone: 'sync' as const, reason: app.rootSupport.reason, enabled: rootIdle && !!behind.length,
      run: () => app.startClone(behind, 'pull') },
    { id: 'switch', label: `Switch ${plural(offRef.length, 'repository', 'repositories')} to checkout ref`, icon: 'branch', tone: 'branch' as const, reason: app.rootSupport.reason, enabled: rootIdle && !!offRef.length,
      run: () => app.startClone(offRef, 'switch') },
    { id: 'push', label: `Push ${plural(pushable.length, 'repository', 'repositories')}`, icon: 'upload', tone: 'sync' as const, reason: app.rootSupport.reason, enabled: folderIdle && !!pushable.length,
      run: () => app.pushRepos(pushable.map(item => ({ path: app.dest(item), name: app.folderOf(item) }))) },
    { id: 'new-branch', label: local.length > 1 ? `New branch in ${local.length} repositories…` : 'New branch…', icon: 'branch', tone: 'branch' as const, reason: app.rootSupport.reason, enabled: folderIdle && !!local.length,
      run: () => app.openBranchDialog(local) },
    { id: 'cleanup', label: local.length > 1 ? `Clean up merged branches in ${local.length} repositories…` : 'Clean up merged branches…', icon: 'trash', tone: 'branch' as const, reason: app.rootSupport.reason, enabled: folderIdle && !!local.length,
      run: () => app.openCleanupDialog(local) },
    ...(items.length === 1 ? [
      { id: 'commit', label: 'Commit changes…', icon: 'check' as const, tone: 'record' as const, reason: app.rootSupport.reason, enabled: folderIdle && !!local.length, run: () => app.openGitDialog('commit', items[0]) },
      { id: 'history', label: 'Show commit history', icon: 'commit' as const, tone: 'inspect' as const, enabled: !!local.length, run: () => historyDrawer.open({ path: app.dest(items[0]), name: app.folderOf(items[0]) }, paletteReturn.element) },
      { id: 'code', label: 'Open repository in VS Code', icon: 'code' as const, enabled: !!local.length, run: () => app.openVscode(app.dest(items[0])) },
    ] : []),
  ];
}

export function shortcut(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>): string | undefined {
  const control = event.ctrlKey || event.metaKey;
  if (control && !event.altKey && event.shiftKey && event.key.toLowerCase() === 'f') return 'search-code';
  if (control && !event.altKey && !event.shiftKey && railShortcut(event.key)) return `rail-${event.key}`;
  if (event.ctrlKey && !event.metaKey && !event.altKey && event.key === 'Tab') return event.shiftKey ? 'tab-previous' : 'tab-next';
  if (control && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'w') return 'tab-close';
  if (control && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 's') return 'editor-save';
  if (!control && !event.altKey && event.key === 'F7') return event.shiftKey ? 'difference-previous' : 'difference-next';
  if (control && event.altKey && !event.shiftKey && event.key === 'ArrowLeft') return 'hunk-left';
  if (control && event.altKey && !event.shiftKey && event.key === 'ArrowRight') return 'hunk-right';
}

export function execute(command: Command) {
  if (!command.enabled) return;
  app.paletteOpen = false;
  Promise.resolve(command.run()).catch(error => app.toast(String(error), 'error'));
}