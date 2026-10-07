<script lang="ts">
  import type { RepoEntry } from '../../../lib/repositories';
  import { app } from '../../../lib/state.svelte';
  import CommitList from '../CommitList.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry, count = 4, onmore }: { entry: RepoEntry; count?: number; onmore?: () => void } = $props();
</script>

<SectionCard title="History" sub="Newest commits">
  {#snippet actions()}{#if onmore}<button class="rf-link" onclick={onmore}>Open ›</button>{/if}{/snippet}
  <CommitList path={app.dest(entry.item)} name={app.folderOf(entry.item)} {count} />
</SectionCard>
