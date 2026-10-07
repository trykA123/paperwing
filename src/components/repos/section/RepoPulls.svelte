<script lang="ts">
  import type { RepoEntry } from '../../../lib/repositories';
  import PullSection from '../../pulls/PullSection.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry }: { entry: RepoEntry } = $props();
</script>

<SectionCard label="Pull requests">
  <PullSection item={entry.item} drawer />
  <p class="hint rf-none">Pull requests belong to the branch this clone has checked out.</p>
</SectionCard>
