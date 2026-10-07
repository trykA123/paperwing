<script lang="ts">
  import { PLACEHOLDER_COPY } from '../../../lib/module-copy';
  import type { RepoEntry } from '../../../lib/repositories';
  import { app } from '../../../lib/state.svelte';
  import Icon from '../../Icon.svelte';
  import SectionCard from './SectionCard.svelte';

  let { entry }: { entry: RepoEntry } = $props();
</script>

<SectionCard title="Actions" sub="Workflow runs">
  <p class="mut">{PLACEHOLDER_COPY.actions?.note}</p>
  <button class="btn small" onclick={() => app.modules.show('actions', entry.host)}><Icon name="actions" />Open the Actions page for {entry.host}</button>
</SectionCard>
