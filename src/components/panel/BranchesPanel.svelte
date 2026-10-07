<script lang="ts">
  import { BRANCH_SECTIONS, branches } from '../../lib/branches.svelte';
  import Icon, { type IconName } from '../Icon.svelte';

  const ICONS: Record<string, IconName> = { cleanup: 'trash', tags: 'tag', stash: 'stash' };
  const count = (id: string) => (id === 'stash' ? branches.dirty.length : branches.cloned.length);
</script>

<div class="sec panel-body">
  <h6>Branches &amp; tags</h6>
  {#each BRANCH_SECTIONS as section (section.id)}
    <button class="nav" class:on={branches.section === section.id} aria-pressed={branches.section === section.id} onclick={() => branches.select(section.id)}>
      <Icon name={ICONS[section.id]} /><span class="lbl">{section.label}</span><span class="cnt">{count(section.id)}</span>
    </button>
  {/each}
  <p class="sb-note">Works on the cloned repositories of the active set.</p>
</div>
