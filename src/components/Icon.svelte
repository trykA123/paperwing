<script lang="ts" module>
  export type IconName = 'branch' | 'tag' | 'commit' | 'search' | 'gear' | 'refresh' | 'folder' | 'code' | 'plus' | 'trash' | 'check' | 'copy' | 'close' | 'panel' | 'theme' | 'chevron' | 'activity' | 'download' | 'info' | 'alert' | 'error' | 'disclosure' | 'folderOpen' | 'maximize' | 'restore' | 'upload' | 'remote' | 'stash' | 'submodule';
  export type IconTone = 'folder' | 'repo' | 'file' | 'branch' | 'tag' | 'commit' | 'ok' | 'warn' | 'err' | 'brand' | 'sync' | 'record' | 'inspect' | 'danger';
</script>

<script lang="ts">
  let { name, size = 14, tone }: { name: IconName; size?: number; tone?: IconTone } = $props();
</script>

<svg class="i {tone ? `tone-${tone}` : ''}" width={size} height={size} viewBox="0 0 16 16" aria-hidden="true">
  {#if name === 'branch'}
    <circle cx="4" cy="3" r="1.6" /><circle cx="4" cy="13" r="1.6" /><circle cx="12" cy="4.5" r="1.6" /><path d="M4 4.6v6.8M12 6.1c0 3.4-8 2.2-8 5.3" />
  {:else if name === 'tag'}
    <path d="M2 2.5h5.5l6.5 6.5-5 5-6.5-6.5z" /><circle cx="5.3" cy="5.8" r="1" />
  {:else if name === 'commit'}
    <circle cx="8" cy="8" r="2.6" /><path d="M1 8h4.4M10.6 8H15" />
  {:else if name === 'search'}
    <circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5 14 14" />
  {:else if name === 'gear'}
    <g transform="scale(0.6667)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </g>
  {:else if name === 'refresh'}
    <path d="M13 8a5 5 0 1 1-1.5-3.5M13 2.5v3h-3" />
  {:else if name === 'folder'}
    <path d="M1.5 4h4.5l1.5 1.5h7v7.5h-13z" />
  {:else if name === 'code'}
    <path d="M5.5 4.5 2 8l3.5 3.5M10.5 4.5 14 8l-3.5 3.5" />
  {:else if name === 'plus'}
    <path d="M8 3v10M3 8h10" />
  {:else if name === 'trash'}
    <path d="M3 4.5h10M6 4.5V3h4v1.5M4.5 4.5l.7 9h5.6l.7-9" />
  {:else if name === 'check'}
    <path d="M3 8.5l3 3 7-7" />
  {:else if name === 'copy'}
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" /><path d="M10.5 3.5v-.5a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h.5" />
  {:else if name === 'close'}
    <path d="m4 4 8 8M12 4l-8 8" />
  {:else if name === 'panel'}
    <rect x="2" y="2.5" width="12" height="11" rx="1" /><path d="M6 2.5v11" />
  {:else if name === 'theme'}
    <path d="M12.8 10A5.5 5.5 0 0 1 6 3.2 5.5 5.5 0 1 0 12.8 10Z" />
  {:else if name === 'chevron'}
    <path d="m6 3 5 5-5 5" />
  {:else if name === 'activity'}
    <path d="M1 8h3l2-5 4 10 2-5h3" />
  {:else if name === 'download'}
    <path d="M8 2v9m-4-4 4 4 4-4M3 13h10" />
  {:else if name === 'info'}
    <circle cx="8" cy="8" r="6" /><path d="M8 7.2v3.6M8 5v.1" stroke-linecap="round" />
  {:else if name === 'alert'}
    <path d="M8 2 1.5 13.5h13z" stroke-linejoin="round" /><path d="M8 6.5v3.2M8 11.8v.1" stroke-linecap="round" />
  {:else if name === 'remote'}
    <path d="M4.5 12.5h7a2.8 2.8 0 0 0 .4-5.57A4 4 0 0 0 4.2 7.1 2.8 2.8 0 0 0 4.5 12.5z" stroke-linejoin="round" />
  {:else if name === 'stash'}
    <rect x="2" y="3" width="12" height="3" rx="1" /><path d="M3 6v6.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V6M6.5 9h3" stroke-linecap="round" />
  {:else if name === 'submodule'}
    <rect x="2" y="2" width="8" height="8" rx="1.5" /><rect x="6" y="6" width="8" height="8" rx="1.5" />
  {:else if name === 'upload'}
    <path d="M8 11V2m-4 4 4-4 4 4M3 13h10" stroke-linecap="round" stroke-linejoin="round" />
  {:else if name === 'disclosure'}
    <path d="m6.2 3.8 4.2 4.2-4.2 4.2" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
  {:else if name === 'folderOpen'}
    <path d="M1.8 12.8V3.5h4.2l1.5 1.5h5.5v2.2M1.8 12.8l1.7-5.6H15l-1.7 5.6z" stroke-linejoin="round" />
  {:else if name === 'maximize'}
    <rect x="3" y="3" width="10" height="10" rx="1.5" />
  {:else if name === 'restore'}
    <path d="M5.5 5.5V3.5h7v7h-2M3.5 5.5h7v7h-7z" stroke-linejoin="round" />
  {:else if name === 'error'}
    <circle cx="8" cy="8" r="6" /><path d="m5.8 5.8 4.4 4.4M10.2 5.8l-4.4 4.4" stroke-linecap="round" />
  {/if}
</svg>
