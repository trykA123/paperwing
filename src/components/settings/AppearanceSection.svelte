<script lang="ts">
  import type { Theme } from '../../lib/api';
  import { CODE_FONTS, UI_FONTS } from '../../lib/appearance';

  let { theme = $bindable(), uiFont = $bindable(), codeFont = $bindable() }: {
    theme: Theme; uiFont: string; codeFont: string;
  } = $props();

  const THEMES: { id: Theme; label: string }[] = [
    { id: 'system', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
</script>

<section class="settings-section">
    <div class="section-head"><div class="grow"><h2>Appearance</h2><p class="mut">Theme and fonts.</p></div></div>
    <div class="card setting-list">
      <div class="setting-row">
        <div class="setting-label"><b>Theme</b><small>System follows the Windows light/dark setting and switches with it.</small></div>
        <div class="seg theme-seg">
          {#each THEMES as t (t.id)}
            <button class:on={theme === t.id} onclick={() => (theme = t.id)}>{t.label}</button>
          {/each}
        </div>
      </div>
      <label class="setting-row">
        <span class="setting-label"><b>Interface font</b><small>Menus, labels and buttons.</small></span>
        <select bind:value={uiFont}>{#each UI_FONTS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}</select>
      </label>
      <label class="setting-row">
        <span class="setting-label"><b>Code font</b><small>Branches, paths and SHAs.</small></span>
        <select bind:value={codeFont}>{#each CODE_FONTS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}</select>
      </label>
    </div>
    <div class="preview">
      <div><b>application-feature</b> <span class="mut">demo-project · example-org · 3 changes</span></div>
      <div class="mono"><span class="t-branch">release/2.4</span> · <span class="t-tag">v2.4.1</span> · <span class="t-commit">a1b2c3d4</span> · C:\Dev\repos\example-org</div>
    </div>
  </section>
