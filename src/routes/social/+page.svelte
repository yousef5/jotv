<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import ReelLibrary from '$lib/components/ReelLibrary.svelte';
  import SocialDownloader from '$lib/components/SocialDownloader.svelte';
  import { reels } from '$lib/stores/reels';
  import { mainScrollSnapshot } from '$lib/scroll';

  export const snapshot = mainScrollSnapshot;

  let tab = $derived($page.url.searchParams.get('tab') === 'download' ? 'download' : 'library');

  function setTab(t: 'library' | 'download') {
    goto(t === 'download' ? '/social?tab=download' : '/social', { replaceState: true, noScroll: true, keepFocus: true });
  }
</script>

<div class="social">
  <header class="head">
    <h1>{tab === 'library' ? 'Video library' : 'Download'}</h1>
    <div class="tabs" role="tablist" aria-label="Social">
      <button role="tab" aria-selected={tab === 'library'} class:on={tab === 'library'} onclick={() => setTab('library')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="3.5" y="3.5" width="7" height="17" rx="1.6"/><rect x="13.5" y="3.5" width="7" height="10" rx="1.6"/></svg>
        Library
        {#if $reels.length}<em>{$reels.length}</em>{/if}
      </button>
      <button role="tab" aria-selected={tab === 'download'} class:on={tab === 'download'} onclick={() => setTab('download')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11"/><polyline points="7 10 12 15 17 10"/><path d="M4 19.5h16"/></svg>
        Download
      </button>
    </div>
  </header>

  <!-- Both stay mounted: downloads keep reporting progress while you browse the library -->
  <div hidden={tab !== 'library'}>
    <ReelLibrary />
  </div>
  <div hidden={tab !== 'download'}>
    <SocialDownloader />
  </div>
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 22px;
    margin-bottom: 18px;
  }
  h1 {
    font-size: 1.625rem;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 9px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border);
  }
  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 14px;
    border-radius: 7px;
    background: none;
    color: var(--color-text-muted);
    font-size: 0.875rem;
    font-weight: 700;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .tabs button :global(svg) { width: 17px; height: 17px; }
  .tabs button em {
    font-style: normal;
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-muted);
  }
  .tabs button:hover { color: var(--color-text); }
  .tabs button.on { background: oklch(1 0 0 / 0.1); color: var(--color-text); }
  .tabs button:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
</style>
