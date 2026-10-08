import type { Source, Workspace } from '../api';
import { HOME_MODULE, isModuleVisible, moduleOfView, ownsPage, sectionOnActivate, viewOfModule, type ModuleId } from '../modules';
import { isRepoSection } from '../repo-sections';
import type { View } from '../workspace';
import type { SidebarLayout } from './sidebar.svelte';

export type ModuleHost = {
  readonly ws: Workspace;
  readonly sources: readonly Source[];
  readonly view: View;
  readonly sidebar: SidebarLayout;
  openView: (view: View) => void;
  openCodeSearch: () => void;
};

/** Which module the rail shows: clicks, flyout picks, the saved module at launch and the sidebar following tabs. */
export class ModuleNavigation {
  /** The host a flyout pick chose, or null for every host. */
  host = $state<string | null>(null);

  constructor(private readonly app: ModuleHost) {}

  /** A rail click: fold the sidebar when its module is already showing, otherwise show the module and open its page. */
  open(id: ModuleId) {
    const shell = this.app.ws.shell;
    const showing = id !== 'settings' && this.app.sidebar.shown && shell.section === id;
    if (showing && this.app.view.kind !== 'repo' && (!ownsPage(id) || moduleOfView(this.app.view) === id)) { this.app.sidebar.set(false); return; }
    this.show(id);
  }

  /** Shows a module's sidebar and page, scoped to one host when given; unlike a rail click it never folds the sidebar. */
  show(id: ModuleId, host: string | null = null) {
    this.host = host;
    if (id === 'settings') { this.app.openView({ kind: 'settings' }); return; }
    this.app.ws.shell.section = id;
    this.app.sidebar.reveal(ownsPage(id));
    if (id === 'search') { this.app.openCodeSearch(); return; }
    const view = viewOfModule(id);
    if (view) this.app.openView(view);
  }

  /** The sidebar follows the tab that became active. */
  follow(view: View) {
    const shell = this.app.ws.shell;
    if (view.kind === 'repo') shell.lastRepo = { repoId: view.repoId, section: view.section };
    else if (view.kind === 'repos' || view.kind === 'set') delete shell.lastRepo;
    shell.section = sectionOnActivate(shell.section, moduleOfView(view));
  }

  /** Tabs are not saved, so a saved module with a page opens that page again; Search starts from the home module. */
  restore() {
    const shell = this.app.ws.shell;
    if (shell.section === 'search' || !isModuleVisible(shell.section, this.app.sources)) shell.section = HOME_MODULE;
    const view = viewOfModule(shell.section);
    const last = shell.lastRepo;
    if (shell.section === HOME_MODULE && last && isRepoSection(last.section)) this.app.openView({ kind: 'repo', repoId: last.repoId, section: last.section });
    else if (view && shell.section !== HOME_MODULE) this.app.openView(view);
  }
}
