<script lang="ts">
  import type { RepoEntry } from '../../../lib/repositories';
  import { app } from '../../../lib/state.svelte';
  import HistoryPanel from '../../HistoryPanel.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry }: { entry: RepoEntry } = $props();
</script>

<SectionCard title="History" sub="Working tree, local and origin rails">
  <HistoryPanel path={app.dest(entry.item)} inline />
</SectionCard>
