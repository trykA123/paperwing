<script lang="ts">
  import type { RepoEntry } from '../../../lib/repositories';
  import { app } from '../../../lib/state.svelte';
  import StashSection from '../../stash/StashSection.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry }: { entry: RepoEntry } = $props();
</script>

<SectionCard label="Stash">
  <StashSection path={app.dest(entry.item)} name={app.folderOf(entry.item)} drawer />
</SectionCard>
