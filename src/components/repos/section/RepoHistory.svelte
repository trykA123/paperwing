<script lang="ts">
  import type { RepoEntry } from '../../../lib/repositories';
  import { detailsDrawer } from '../../../lib/details-drawer.svelte';
  import { app } from '../../../lib/state.svelte';
  import HistoryPanel from '../../HistoryPanel.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry }: { entry: RepoEntry } = $props();
  const path = $derived(app.dest(entry.item));
</script>

<SectionCard title="History" sub="Working tree, local and origin rails">
  <HistoryPanel path={path} onpick={(row, opener) => detailsDrawer.open({ kind: 'commit', path, name: app.folderOf(entry.item), row }, opener)} />
</SectionCard>
