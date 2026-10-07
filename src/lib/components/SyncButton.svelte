<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { goto } from '$app/navigation';
  import { syncState, syncPlaylists } from '$lib/stores/sync';
  import type { SyncPhase } from '$lib/stores/sync';

  let now = $state(Date.now());
  let open = $state(false);
  let justFinished = $state(false);
  let lastVersion = $syncState.version;
  let phaseStart = $state(Date.now());
  let lastPhase: SyncPhase | null = null;

  const STEPS: { phase: SyncPhase; label: string }[] = [
    { phase: 'connect', label: 'Connect to the server' },
    { phase: 'download', label: 'Download channels, movies & series' },
    { phase: 'process', label: 'Organize categories' },
    { phase: 'save', label: 'Save to your library' },
  ];
  const ORDER: SyncPhase[] = ['connect', 'download', 'process', 'save', 'done'];

  const nf = new Intl.NumberFormat('en-US');

  onMount(() => {
    // 1s while working (elapsed clock, download creep), otherwise every 30s
    const tick = setInterval(() => {
      if ($syncState.running || open) now = Date.now();
    }, 1000);
    const slow = setInterval(() => (now = Date.now()), 30_000);
    return () => {
      clearInterval(tick);
      clearInterval(slow);
    };
  });

  // Open the panel when a sync starts (launch sync included)
  $effect(() => {
    if ($syncState.running) open = true;
  });

  $effect(() => {
    const p = $syncState.phase;
    if (p !== lastPhase) {
      lastPhase = p;
      phaseStart = Date.now();
    }
  });

  $effect(() => {
    const v = $syncState.version;
    if (v !== lastVersion) {
      lastVersion = v;
      now = Date.now();
      if (!$syncState.error) {
        justFinished = true;
        const t = setTimeout(() => (justFinished = false), 4000);
        return () => clearTimeout(t);
      }
    }
  });

  let phaseIndex = $derived($syncState.phase ? ORDER.indexOf($syncState.phase) : -1);

  // The download reports no byte progress; ease through its share so the bar
  // keeps moving without ever claiming to be done
  let shownProgress = $derived.by(() => {
    const p = $syncState.progress;
    if (p === null) return null;
    if ($syncState.phase !== 'download') return p;
    const secs = (now - phaseStart) / 1000;
    return Math.max(p, 0.03 + 0.55 * (1 - Math.exp(-secs / 70)));
  });
  let pct = $derived(shownProgress === null ? null : Math.round(shownProgress * 100));

  let summary = $derived($syncState.summary);
  let addedTotal = $derived(summary ? summary.added_live + summary.added_vod + summary.added_series : 0);

  function relative(ts: number | null): string {
    if (!ts) return 'Not synced yet';
    const s = Math.max(0, (now - ts) / 1000);
    if (s < 60) return 'Updated just now';
    if (s < 3600) return `Updated ${Math.floor(s / 60)}m ago`;
    if (s < 86400) return `Updated ${Math.floor(s / 3600)}h ago`;
    return `Updated ${Math.floor(s / 86400)}d ago`;
  }

  function clock(ms: number): string {
    const s = Math.max(0, Math.floor(ms / 1000));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  }

  let label = $derived(
    $syncState.running
      ? pct ? `Updating ${pct}%` : 'Updating…'
      : justFinished
        ? 'Up to date'
        : $syncState.error
          ? 'Update failed'
          : relative($syncState.lastSyncedAt),
  );

  function stepDetail(phase: SyncPhase): string {
    if (phase !== $syncState.phase) return '';
    const { current, total, stage } = $syncState;
    if (phase === 'save' && total) return `${nf.format(current)} / ${nf.format(total)}`;
    if (phase === 'download') return `${clock(now - phaseStart)} · the longest step`;
    if (phase === 'process') return stage.replace(/\.\.\.$/, '');
    return '';
  }

  function onClick() {
    if ($syncState.running) {
      open = !open;
    } else if (open && !$syncState.error) {
      open = false;
    } else {
      open = true;
      syncPlaylists();
    }
  }

  function see(type: 'vod' | 'series') {
    open = false;
    goto(`/browse?type=${type}`);
  }
</script>

<div class="wrap">
  <button
    class="sync"
    class:running={$syncState.running}
    class:done={justFinished}
    class:failed={!!$syncState.error && !$syncState.running}
    aria-expanded={open}
    aria-busy={$syncState.running}
    title={$syncState.running ? 'Show progress' : 'Refresh library from server'}
    onclick={onClick}
  >
    <span class="icon" aria-hidden="true">
      {#if justFinished}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 12.5 10 17 19 7"/></svg>
      {:else}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 01-15.5 6.2L3 15.5"/><path d="M3 12a9 9 0 0115.5-6.2L21 8.5"/><polyline points="21 3 21 8.5 15.5 8.5"/><polyline points="3 21 3 15.5 8.5 15.5"/></svg>
      {/if}
    </span>
    <span class="label">{label}</span>
    {#if $syncState.running}
      <span class="bar" aria-hidden="true"><span class="fill" style:width="{pct ?? 2}%"></span></span>
    {/if}
  </button>

  {#if open && ($syncState.running || summary || $syncState.error)}
    <div class="panel" role="status" aria-live="polite" transition:fly={{ y: -6, duration: 160 }}>
      <header class="panel-head">
        <div>
          <strong>
            {#if $syncState.running}Updating your library{:else if $syncState.error}Update failed{:else}Library updated{/if}
          </strong>
          <span>
            {$syncState.playlistName}
            {#if $syncState.running && $syncState.startedAt} · {clock(now - $syncState.startedAt)}{/if}
            {#if !$syncState.running && $syncState.lastSyncedAt && !$syncState.error} · {relative($syncState.lastSyncedAt).replace('Updated ', '')}{/if}
          </span>
        </div>
        <button class="close" onclick={() => (open = false)} aria-label="Close">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </header>

      {#if $syncState.running}
        <ol class="steps">
          {#each STEPS as step, i (step.phase)}
            {@const state = i < phaseIndex ? 'done' : i === phaseIndex ? 'active' : 'todo'}
            <li class={state}>
              <span class="dot" aria-hidden="true">
                {#if state === 'done'}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 12.5 10 17 19 7"/></svg>
                {:else if state === 'active'}
                  <span class="spin"></span>
                {/if}
              </span>
              <span class="step-text">
                <b>{step.label}</b>
                {#if stepDetail(step.phase)}<small>{stepDetail(step.phase)}</small>{/if}
              </span>
            </li>
          {/each}
        </ol>
        <div class="progress">
          <span class="track"><span class="fill" style:width="{pct ?? 2}%"></span></span>
          <span class="pct">{pct ?? 0}%</span>
        </div>
        <p class="note">You can keep watching and browsing while this runs.</p>
      {:else if $syncState.error}
        <p class="err">{$syncState.error}</p>
        <div class="actions">
          <button class="primary" onclick={() => syncPlaylists()}>Try again</button>
        </div>
      {:else if summary}
        {#if addedTotal || summary.removed}
          <ul class="changes">
            {#if summary.added_vod}<li><b>+{nf.format(summary.added_vod)}</b> {summary.added_vod === 1 ? 'movie' : 'movies'}</li>{/if}
            {#if summary.added_series}<li><b>+{nf.format(summary.added_series)}</b> series</li>{/if}
            {#if summary.added_live}<li><b>+{nf.format(summary.added_live)}</b> {summary.added_live === 1 ? 'channel' : 'channels'}</li>{/if}
            {#if summary.removed}<li class="minus"><b>−{nf.format(summary.removed)}</b> removed</li>{/if}
          </ul>
        {:else}
          <p class="note">Everything was already up to date. {nf.format(summary.updated)} titles checked.</p>
        {/if}
        {#if summary.added_vod || summary.added_series}
          <div class="actions">
            {#if summary.added_vod}<button class="primary" onclick={() => see('vod')}>See new movies</button>{/if}
            {#if summary.added_series}<button onclick={() => see('series')}>See new series</button>{/if}
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap { position: relative; }

  .sync {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 14px 0 11px;
    border-radius: 999px;
    overflow: hidden;
    background: oklch(0.13 0.004 25 / 0.55);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.14);
    backdrop-filter: blur(14px);
    -webkit-backdrop-filter: blur(14px);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    transition: background 200ms var(--ease-out), box-shadow 200ms var(--ease-out);
  }
  .sync:hover { background: oklch(0.22 0.005 25 / 0.75); box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.28); }
  .sync:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .icon { display: grid; place-items: center; width: 18px; height: 18px; }
  .icon :global(svg) { width: 18px; height: 18px; }
  .sync:hover:not(.running) .icon :global(svg) { transform: rotate(90deg); transition: transform 400ms var(--ease-out); }
  .running .icon :global(svg) { animation: spin 900ms linear infinite; }
  .done { box-shadow: inset 0 0 0 1px oklch(0.76 0.13 165 / 0.5); }
  .done .icon { color: var(--color-accent-green); }
  .failed { box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.6); }
  .failed .icon { color: var(--color-accent-soft); }

  .bar { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: oklch(1 0 0 / 0.08); }
  .fill { display: block; height: 100%; background: var(--color-accent); transition: width 600ms var(--ease-out); }

  /* Panel */
  .panel {
    position: absolute;
    top: calc(100% + 10px);
    right: 0;
    z-index: 30;
    width: 340px;
    padding: 16px;
    border-radius: 12px;
    background: oklch(0.18 0.005 25 / 0.96);
    backdrop-filter: blur(18px);
    -webkit-backdrop-filter: blur(18px);
    box-shadow: 0 24px 60px -16px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(1 0 0 / 0.08);
    color: var(--color-text);
    text-align: start;
  }
  .panel-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
  .panel-head strong { display: block; font-size: 0.9375rem; font-weight: 800; }
  .panel-head span { font-size: 0.75rem; color: var(--color-text-muted); font-variant-numeric: tabular-nums; }
  .close {
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: none;
    color: var(--color-text-muted);
  }
  .close:hover { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  .close :global(svg) { width: 14px; height: 14px; }

  .steps { list-style: none; display: flex; flex-direction: column; gap: 10px; margin-bottom: 14px; }
  .steps li { display: flex; align-items: flex-start; gap: 11px; }
  .dot {
    flex-shrink: 0;
    width: 20px;
    height: 20px;
    margin-top: 1px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    box-shadow: inset 0 0 0 2px oklch(1 0 0 / 0.18);
  }
  .done .dot { background: var(--color-accent-green); box-shadow: none; color: oklch(0.14 0.004 25); }
  .done .dot :global(svg) { width: 12px; height: 12px; }
  .active .dot { box-shadow: none; }
  .spin {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2.5px solid oklch(1 0 0 / 0.15);
    border-top-color: var(--color-accent);
    animation: spin 800ms linear infinite;
  }
  .step-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .step-text b { font-size: 0.8125rem; font-weight: 600; }
  .todo .step-text b { color: var(--color-text-muted); font-weight: 500; }
  .active .step-text b { font-weight: 700; }
  .step-text small { font-size: 0.75rem; color: var(--color-text-muted); font-variant-numeric: tabular-nums; }

  .progress { display: flex; align-items: center; gap: 10px; }
  .track { flex: 1; height: 6px; border-radius: 3px; background: oklch(1 0 0 / 0.1); overflow: hidden; }
  .track .fill { border-radius: 3px; }
  .pct { min-width: 36px; text-align: end; font-size: 0.8125rem; font-weight: 800; font-variant-numeric: tabular-nums; }

  .note { margin-top: 10px; font-size: 0.75rem; color: var(--color-text-muted); line-height: 1.5; }
  .err { font-size: 0.8125rem; color: var(--color-accent-soft); line-height: 1.5; }

  .changes {
    list-style: none;
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
  }
  .changes li {
    padding: 10px 12px;
    border-radius: 8px;
    background: oklch(1 0 0 / 0.05);
    font-size: 0.8125rem;
    color: var(--color-text-muted);
  }
  .changes b { display: block; font-size: 1.25rem; font-weight: 800; color: var(--color-accent-green); font-variant-numeric: tabular-nums; }
  .changes .minus b { color: var(--color-text-muted); }

  .actions { display: flex; gap: 8px; margin-top: 14px; }
  .actions button {
    flex: 1;
    height: 36px;
    border-radius: 6px;
    background: oklch(1 0 0 / 0.1);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
  }
  .actions button:hover { background: oklch(1 0 0 / 0.16); }
  .actions .primary { background: var(--color-text); color: oklch(0.14 0.004 25); }
  .actions .primary:hover { background: oklch(0.85 0.004 25); }

  @keyframes spin { to { transform: rotate(360deg); } }

  @media (prefers-reduced-motion: reduce) {
    .running .icon :global(svg), .spin { animation: none; }
  }
</style>
