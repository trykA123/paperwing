<script lang="ts">
  let { rows = 6, height = 56 }: { rows?: number; height?: number } = $props();
  const WIDTHS = [62, 48, 71, 55];
</script>

<div class="skeleton" aria-hidden="true">
  {#each Array(rows) as _, index}
    <div class="skel-row" style:height="{height}px">
      <i class="skel" style:width="18px" style:height="18px"></i>
      <i class="skel" style:width="{WIDTHS[index % WIDTHS.length]}%"></i>
      <i class="skel" style:width="72px"></i>
    </div>
  {/each}
</div>
