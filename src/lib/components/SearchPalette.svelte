<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { fade, scale } from 'svelte/transition';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { globalSearch, searchWarmup } from '$lib/tauri';
  import type { GlobalSearchResult, MediaChannel } from '$lib/tauri';
  import { searchPalette, openSearch, closeSearch } from '$lib/stores/search';
  import { pendingOpen } from '$lib/stores/playlists';
  import { overlayOpen } from '$lib/stores/live';
  import { splitTitle } from '$lib/playback';

  type Filter = 'all' | 'live' | 'vod' | 'series';

  const RECENT_KEY = 'jotv.recentSearches';
  // Pages with their own Ctrl+K search keep it
  const LOCAL_SEARCH = ['/live', '/browse'];

  let query = $state('');
  let filter = $state<Filter>('all');
  let result = $state<GlobalSearchResult | null>(null);
  let loading = $state(false);
  let active = $state(0);
  let recent = $state<string[]>(readRecent());
  let inputEl = $state<HTMLInputElement | undefined>();
  let listEl = $state<HTMLDivElement | undefined>();
  let failed = $state<Set<number>>(new Set());

  let token = 0;
  let debounce: ReturnType<typeof setTimeout> | undefined;

  const nf = new Intl.NumberFormat('en-US');

  let open = $derived($searchPalette.open);

  // Flat list in display order, for ↑/↓ and Enter
  let flat = $derived.by(() => {
    if (!result) return [] as MediaChannel[];
    const per = filter === 'all' ? { live: 6, vod: 12, series: 12 } : { live: 40, vod: 40, series: 40 };
    const out: MediaChannel[] = [];
    if (filter === 'all' || filter === 'live') out.push(...result.live.slice(0, per.live));
    if (filter === 'all' || filter === 'vod') out.push(...result.vod.slice(0, per.vod));
    if (filter === 'all' || filter === 'series') out.push(...result.series.slice(0, per.series));
    return out;
  });
  let live = $derived(flat.filter((c) => c.content_type === 'live'));
  let vod = $derived(flat.filter((c) => c.content_type === 'vod'));
  let series = $derived(flat.filter((c) => c.content_type === 'series'));
  let total = $derived(result ? result.counts.live + result.counts.vod + result.counts.series : 0);

  function readRecent(): string[] {
    try {
      return JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]').slice(0, 8);
    } catch {
      return [];
    }
  }

  function remember(q: string) {
    const term = q.trim();
    if (!term) return;
    recent = [term, ...recent.filter((r) => r.toLowerCase() !== term.toLowerCase())].slice(0, 8);
    try { localStorage.setItem(RECENT_KEY, JSON.stringify(recent)); } catch {}
  }

  // Opening: take the starting query, focus, and warm the index. Only `open`
  // is tracked; reading `query` here would reset the box on every keystroke.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      overlayOpen.set(true);
      query = $searchPalette.query;
      filter = 'all';
      active = 0;
      searchWarmup().catch(() => {});
      if (query) run(query);
    });
    tick().then(() => inputEl?.focus());
    return () => overlayOpen.set(false);
  });

  function onInput() {
    clearTimeout(debounce);
    const term = query.trim();
    if (!term) {
      result = null;
      loading = false;
      return;
    }
    loading = true;
    debounce = setTimeout(() => run(term), 90);
  }

  async function run(term: string) {
    const t = ++token;
    loading = true;
    try {
      const r = await globalSearch(term, 40);
      if (t !== token) return;
      result = r;
      active = 0;
      listEl?.scrollTo({ top: 0 });
    } catch {
      if (t === token) result = null;
    } finally {
      if (t === token) loading = false;
    }
  }

  function choose(c: MediaChannel) {
    remember(query);
    closeSearch();
    if (c.content_type === 'live') {
      pendingOpen.set({ playlistId: c.playlist_id, contentType: 'live', channel: c, action: 'play' });
      goto('/live');
    } else {
      goto(`/title?id=${c.id}`);
    }
  }

  function seeAll(type: 'live' | 'vod' | 'series') {
    remember(query);
    const q = encodeURIComponent(query.trim());
    closeSearch();
    goto(type === 'live' ? `/live?q=${q}` : `/browse?type=${type}&q=${q}`);
  }

  async function moveActive(delta: number) {
    if (!flat.length) return;
    active = (active + delta + flat.length) % flat.length;
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if (!open) {
      const local = LOCAL_SEARCH.includes($page.url.pathname);
      if ((e.key === 'k' && (e.ctrlKey || e.metaKey) && !local) || (e.key === '/' && !typing && $page.url.pathname === '/')) {
        e.preventDefault();
        openSearch();
      }
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      closeSearch();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      moveActive(1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      moveActive(-1);
    } else if (e.key === 'Enter' && flat[active]) {
      e.preventDefault();
      choose(flat[active]);
    } else if (e.key === 'Tab') {
      e.preventDefault();
      const order: Filter[] = ['all', 'live', 'vod', 'series'];
      filter = order[(order.indexOf(filter) + (e.shiftKey ? 3 : 1)) % 4];
      active = 0;
    }
  }

  function indexOf(c: MediaChannel): number {
    return flat.indexOf(c);
  }
</script>

<svelte:window onkeydown={onKey} />

{#if open}
  <div class="backdrop" transition:fade={{ duration: 140 }} onclick={closeSearch} aria-hidden="true"></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Search" transition:scale={{ start: 0.97, duration: 160 }}>
    <label class="field">
      {#if loading}
        <span class="spinner" aria-hidden="true"></span>
      {:else}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
      {/if}
      <input
        bind:this={inputEl}
        bind:value={query}
        oninput={onInput}
        placeholder="Search channels, movies and series"
        aria-label="Search everything"
        aria-controls="search-results"
        aria-activedescendant={flat[active] ? `sr-${flat[active].id}` : undefined}
        autocomplete="off"
        spellcheck="false"
        dir="auto"
      />
      {#if query}
        <button class="clear" onclick={() => { query = ''; result = null; inputEl?.focus(); }} aria-label="Clear">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      {/if}
      <kbd>Esc</kbd>
    </label>

    {#if result && query.trim()}
      <div class="filters" role="tablist" aria-label="Filter results">
        {#each [['all', 'All', total], ['live', 'Channels', result.counts.live], ['vod', 'Movies', result.counts.vod], ['series', 'Series', result.counts.series]] as [key, label, count] (key)}
          <button
            role="tab"
            aria-selected={filter === key}
            class:on={filter === key}
            disabled={key !== 'all' && !count}
            onclick={() => { filter = key as Filter; active = 0; inputEl?.focus(); }}
          >
            {label}<span>{nf.format(count as number)}</span>
          </button>
        {/each}
        <span class="speed">{result.elapsed_ms < 1 ? '<1' : Math.round(result.elapsed_ms)} ms</span>
      </div>
    {/if}

    <div class="results" id="search-results" role="listbox" bind:this={listEl}>
      {#if !query.trim()}
        <div class="start">
          {#if recent.length}
            <h3>Recent searches</h3>
            <div class="recent">
              {#each recent as r (r)}
                <button onclick={() => { query = r; run(r); inputEl?.focus(); }} dir="auto">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 15 14"/></svg>
                  {r}
                </button>
              {/each}
              <button class="forget" onclick={() => { recent = []; try { localStorage.removeItem(RECENT_KEY); } catch {} }}>Clear</button>
            </div>
          {/if}
          <h3>Try</h3>
          <p class="hint">A channel like <b>bein sports</b>, a movie like <b>spider-man</b>, or an Arabic title like <b>الاهلي</b>. Spelling variants (أ/ا, ة/ه) match automatically.</p>
          <dl class="keys">
            <div><dt>↑ ↓</dt><dd>Move</dd></div>
            <div><dt>Enter</dt><dd>Open</dd></div>
            <div><dt>Tab</dt><dd>Next filter</dd></div>
            <div><dt>Esc</dt><dd>Close</dd></div>
          </dl>
        </div>
      {:else if result && !flat.length && !loading}
        <div class="none">
          <strong>Nothing matches “{query.trim()}”</strong>
          <span>Try fewer words, or part of the name.</span>
        </div>
      {:else if result}
        {#if live.length}
          <section>
            <header>
              <h3>Channels</h3>
              {#if result.counts.live > live.length}<button onclick={() => seeAll('live')}>See all {nf.format(result.counts.live)}</button>{/if}
            </header>
            <div class="rows">
              {#each live as c (c.id)}
                {@const i = indexOf(c)}
                <button class="row" class:active={i === active} id={`sr-${c.id}`} data-index={i} role="option" aria-selected={i === active} onmouseenter={() => (active = i)} onclick={() => choose(c)}>
                  <span class="logo">
                    {#if c.logo_url && !failed.has(c.id)}
                      <img src={c.logo_url} alt="" loading="lazy" onerror={() => (failed = new Set(failed).add(c.id))} />
                    {:else}{c.name.trim().charAt(0).toUpperCase()}{/if}
                  </span>
                  <span class="text"><b dir="auto">{c.name}</b><small dir="auto">{c.group_name}</small></span>
                  <span class="tag live"><i></i>Live</span>
                </button>
              {/each}
            </div>
          </section>
        {/if}

        {#each [['vod', 'Movies', vod], ['series', 'Series', series]] as [key, label, list] (key)}
          {#if (list as MediaChannel[]).length}
            <section>
              <header>
                <h3>{label}</h3>
                {#if result.counts[key as 'vod' | 'series'] > (list as MediaChannel[]).length}
                  <button onclick={() => seeAll(key as 'vod' | 'series')}>See all {nf.format(result.counts[key as 'vod' | 'series'])}</button>
                {/if}
              </header>
              <div class="posters">
                {#each list as MediaChannel[] as c (c.id)}
                  {@const i = indexOf(c)}
                  {@const t = splitTitle(c.name)}
                  <button class="poster" class:active={i === active} id={`sr-${c.id}`} data-index={i} role="option" aria-selected={i === active} onmouseenter={() => (active = i)} onclick={() => choose(c)}>
                    <span class="art">
                      {#if c.logo_url && !failed.has(c.id)}
                        <img src={c.logo_url} alt="" loading="lazy" onerror={() => (failed = new Set(failed).add(c.id))} />
                      {:else}<span class="art-empty" dir="auto">{t.title}</span>{/if}
                      {#if c.rating}<span class="score">★ {c.rating.toFixed(1)}</span>{/if}
                    </span>
                    <b dir="auto">{t.title}</b>
                    <small>{c.year ?? t.year ?? ''}</small>
                  </button>
                {/each}
              </div>
            </section>
          {/if}
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 900;
    background: oklch(0.08 0.003 25 / 0.72);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
  }

  .palette {
    position: fixed;
    top: 8vh;
    left: 50%;
    z-index: 901;
    width: min(920px, calc(100vw - 48px));
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    translate: -50% 0;
    border-radius: 16px;
    background: oklch(0.17 0.005 25);
    box-shadow: 0 40px 100px -20px oklch(0 0 0 / 0.9), 0 0 0 1px oklch(1 0 0 / 0.09);
    overflow: hidden;
  }

  .field {
    position: relative;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 68px;
    padding: 0 18px 0 22px;
    border-bottom: 1px solid oklch(1 0 0 / 0.07);
  }
  .field > :global(svg) { width: 22px; height: 22px; flex-shrink: 0; color: var(--color-text-muted); }
  .field input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    background: none;
    font-size: 1.25rem;
    font-weight: 600;
    color: var(--color-text);
  }
  .field input:focus { border: none; }
  .field input::placeholder { color: oklch(0.55 0.005 25); font-weight: 500; }
  .field kbd {
    padding: 3px 8px;
    border-radius: 5px;
    background: oklch(1 0 0 / 0.08);
    color: var(--color-text-muted);
    font-family: inherit;
    font-size: 0.6875rem;
    font-weight: 700;
  }
  .clear {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(1 0 0 / 0.1);
    color: var(--color-text);
  }
  .clear :global(svg) { width: 13px; height: 13px; }

  .filters {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px 18px;
    border-bottom: 1px solid oklch(1 0 0 / 0.05);
  }
  .filters button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 13px;
    border-radius: 16px;
    background: oklch(1 0 0 / 0.06);
    color: oklch(0.85 0.004 25);
    font-size: 0.8125rem;
    font-weight: 700;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .filters button span { font-size: 0.75rem; opacity: 0.6; font-variant-numeric: tabular-nums; }
  .filters button:hover:not(:disabled) { background: oklch(1 0 0 / 0.12); }
  .filters button.on { background: var(--color-text); color: oklch(0.14 0.004 25); }
  .filters button:disabled { opacity: 0.35; }
  .speed { margin-left: auto; font-size: 0.6875rem; color: var(--color-text-muted); font-variant-numeric: tabular-nums; }

  .results { overflow-y: auto; padding: 6px 10px 18px; overscroll-behavior: contain; }

  section { padding: 10px 8px 4px; }
  section header { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 8px; }
  h3 {
    font-size: 0.6875rem;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  section header button { background: none; color: var(--color-accent-soft); font-size: 0.8125rem; font-weight: 700; }
  section header button:hover { color: var(--color-text); }

  .rows { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 4px; }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 10px;
    background: none;
    color: var(--color-text);
    text-align: start;
  }
  .row.active { background: oklch(1 0 0 / 0.09); box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.12); }
  .logo {
    flex-shrink: 0;
    width: 56px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    background: oklch(0.24 0.005 25);
    font-weight: 800;
    color: var(--color-text-muted);
    overflow: hidden;
  }
  .logo img { width: 100%; height: 100%; object-fit: contain; padding: 4px; }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .text b { font-size: 0.875rem; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .text small { font-size: 0.75rem; color: var(--color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tag.live {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border-radius: 3px;
    background: var(--color-accent);
    color: var(--color-on-accent);
    font-size: 0.625rem;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .tag i { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

  .posters { display: grid; grid-template-columns: repeat(auto-fill, minmax(118px, 1fr)); gap: 14px 10px; }
  .poster { display: flex; flex-direction: column; gap: 5px; padding: 0; background: none; color: var(--color-text); text-align: start; min-width: 0; }
  .art {
    position: relative;
    display: block;
    aspect-ratio: 2 / 3;
    border-radius: 8px;
    overflow: hidden;
    background: var(--color-card);
    transition: transform 160ms var(--ease-out), box-shadow 160ms var(--ease-out);
  }
  .art img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .art-empty {
    display: flex;
    align-items: flex-end;
    width: 100%;
    height: 100%;
    padding: 10px;
    font-size: 0.8125rem;
    font-weight: 800;
    background: radial-gradient(120% 80% at 100% 0%, oklch(0.42 0.16 27 / 0.5), transparent 60%), var(--color-card);
  }
  .poster.active .art { transform: translateY(-3px); box-shadow: 0 0 0 2px var(--color-text), 0 16px 30px -12px oklch(0 0 0 / 0.8); }
  .score {
    position: absolute;
    top: 6px;
    left: 6px;
    padding: 2px 6px;
    border-radius: 4px;
    background: oklch(0.12 0.004 25 / 0.82);
    font-size: 0.6875rem;
    font-weight: 800;
    color: var(--color-accent-yellow);
  }
  .poster b { font-size: 0.8125rem; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .poster small { margin-top: -4px; font-size: 0.75rem; color: var(--color-text-muted); }

  .start { padding: 18px 12px 8px; display: flex; flex-direction: column; gap: 12px; }
  .recent { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 8px; }
  .recent button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 34px;
    padding: 0 14px 0 11px;
    border-radius: 17px;
    background: oklch(1 0 0 / 0.07);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 600;
  }
  .recent button :global(svg) { width: 14px; height: 14px; color: var(--color-text-muted); }
  .recent button:hover { background: oklch(1 0 0 / 0.13); }
  .recent .forget { background: none; color: var(--color-text-muted); }
  .hint { font-size: 0.875rem; color: var(--color-text-muted); line-height: 1.6; max-width: 70ch; }
  .hint b { color: var(--color-text); }
  .keys { display: flex; flex-wrap: wrap; gap: 8px 18px; margin-top: 4px; font-size: 0.8125rem; color: var(--color-text-muted); }
  .keys div { display: flex; align-items: center; gap: 7px; }
  .keys dt { padding: 2px 7px; border-radius: 4px; background: oklch(1 0 0 / 0.08); color: var(--color-text); font-size: 0.6875rem; font-weight: 700; }

  .none { padding: 48px 20px; display: flex; flex-direction: column; align-items: center; gap: 6px; text-align: center; }
  .none strong { font-size: 1.0625rem; }
  .none span { font-size: 0.875rem; color: var(--color-text-muted); }

  .spinner {
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    border-radius: 50%;
    border: 2.5px solid oklch(1 0 0 / 0.12);
    border-top-color: var(--color-accent);
    animation: spin 700ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (prefers-reduced-motion: reduce) {
    .spinner { animation: none; }
    .art { transition: none; }
  }
</style>
