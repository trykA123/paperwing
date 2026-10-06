<script lang="ts">
  let { busy = false, size = 24 }: { busy?: boolean; size?: number } = $props();
  let settled = $state(false);

  $effect(() => { if (busy) settled = true; });
</script>

<svg class="logo skein-mark" class:busy class:settled width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
  <rect width="64" height="64" rx="15" fill="#006eb8" />
  <path d="M14 21H24C34 21 30 43 40 43H50" fill="none" stroke="#a7d4e4" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" />
  <circle cx="14" cy="21" r="6" fill="#a7d4e4" />
  <g class="travel"><circle class="pulse" cx="50" cy="43" r="6.5" fill="#fff200" onanimationend={() => (settled = true)} /></g>
</svg>

<style>
  .skein-mark { flex: none; }
  .travel { animation: travel 1.3s linear both; }
  .pulse { transform-box: fill-box; transform-origin: center; animation: land 320ms ease-out 1.3s both; }
  .settled .travel, .settled .pulse { animation: none; }
  .busy .travel { animation: loop 1.5s linear infinite; }
  .busy .pulse { animation: none; }

  @keyframes land { 50% { transform: scale(1.35); } }
  @keyframes travel {
    0% { transform: translate(-36px, -22px); }
    5% { transform: translate(-35.8px, -22px); }
    10% { transform: translate(-35px, -22px); }
    15% { transform: translate(-33.7px, -22px); }
    20% { transform: translate(-31.9px, -22px); }
    25% { transform: translate(-29.6px, -22px); }
    30% { transform: translate(-26.7px, -22px); }
    35% { transform: translate(-23.5px, -21.4px); }
    40% { transform: translate(-20.8px, -18.8px); }
    45% { transform: translate(-19.2px, -15.1px); }
    50% { transform: translate(-18px, -11px); }
    55% { transform: translate(-16.8px, -7px); }
    60% { transform: translate(-15.2px, -3.2px); }
    65% { transform: translate(-12.6px, -0.6px); }
    70% { transform: translate(-9.3px, 0px); }
    75% { transform: translate(-6.4px, 0px); }
    80% { transform: translate(-4px, 0px); }
    85% { transform: translate(-2.2px, 0px); }
    90% { transform: translate(-1px, 0px); }
    95% { transform: translate(-0.2px, 0px); }
    100% { transform: translate(0px, 0px); }
  }
  @keyframes loop {
    0% { transform: translate(-36px, -22px); }
    5% { transform: translate(-33.5px, -22px); }
    10% { transform: translate(-31px, -22px); }
    15% { transform: translate(-28.5px, -22px); }
    20% { transform: translate(-26.1px, -22px); }
    25% { transform: translate(-23.7px, -21.5px); }
    30% { transform: translate(-21.8px, -20.1px); }
    35% { transform: translate(-20.4px, -18px); }
    40% { transform: translate(-19.4px, -15.7px); }
    45% { transform: translate(-18.7px, -13.4px); }
    50% { transform: translate(-18px, -11px); }
    55% { transform: translate(-17.3px, -8.6px); }
    60% { transform: translate(-16.6px, -6.3px); }
    65% { transform: translate(-15.6px, -4px); }
    70% { transform: translate(-14.3px, -2px); }
    75% { transform: translate(-12.3px, -0.5px); }
    80% { transform: translate(-9.9px, 0px); }
    85% { transform: translate(-7.4px, 0px); }
    90% { transform: translate(-4.9px, 0px); }
    95% { transform: translate(-2.5px, 0px); }
    100% { transform: translate(0px, 0px); }
  }

  @media (prefers-reduced-motion: reduce) {
    .travel, .pulse, .busy .travel { animation: none; }
  }
</style>
