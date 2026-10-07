<script lang="ts">
  import { SIDEBAR_QUEUES } from '../../lib/pull-queue';
  import { pullQueue } from '../../lib/pull-queue.svelte';
  import Icon from '../Icon.svelte';
</script>

<label class="gsearch">
  <Icon name="search" />
  <input placeholder="Filter pull requests…" aria-label="Filter pull requests" value={pullQueue.query} oninput={event => pullQueue.search(event.currentTarget.value)} spellcheck="false" />
</label>

<div class="sec">
  <h6>Queues</h6>
  {#each SIDEBAR_QUEUES as queue (queue.id)}
    <button class="nav" class:on={pullQueue.queue === queue.id} aria-pressed={pullQueue.queue === queue.id} disabled={!!queue.unavailable} title={queue.unavailable} onclick={() => pullQueue.select(queue.id)}>
      <span class="lbl">{queue.label}</span><span class="cnt" class:acc={queue.id === 'review' && pullQueue.counts.review > 0}>{queue.unavailable ? '–' : pullQueue.counts[queue.id]}</span>
    </button>
  {/each}
</div>
