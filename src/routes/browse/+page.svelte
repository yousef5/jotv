<script lang="ts" module>
  import type { Channel } from '$lib/tauri';

  /** Grid item: search results have no rating/year, list results do */
  type Tile = Channel & { rating?: number | null; year?: number | null };

  // Last loaded list, so going back restores every page that infinite scroll fetched
  let listCache: { key: string; items: Tile[]; hasMore: boolean } | null = null;
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { browseMedia, getMediaFacets, getGroupsByType, searchChannelsInPlaylist } from '$lib/tauri';
  import type { ChannelGroup, MediaSort, MediaFacets } from '$lib/tauri';
  import { playlists, loadPlaylists, RECENTLY_ADDED_GROUP } from '$lib/stores/playlists';
  import { splitTitle } from '$lib/playback';
  import { syncState } from '$lib/stores/sync';
  import { mainScrollSnapshot } from '$lib/scroll';

  export const snapshot = mainScrollSnapshot;

  const PAGE = 60;

  type Kind = 'vod' | 'series';

  let pid = $derived(parseInt($page.url.searchParams.get('pid') ?? '') || $playlists[0]?.id || 0);
  let kind = $derived<Kind>($page.url.searchParams.get('type') === 'series' ? 'series' : 'vod');
  let group = $derived($page.url.searchParams.get('group') ?? RECENTLY_ADDED_GROUP);
  let q = $derived(($page.url.searchParams.get('q') ?? '').trim());
  let sort = $derived<MediaSort>((['rating', 'year', 'name'].includes($page.url.searchParams.get('sort') ?? '') ? $page.url.searchParams.get('sort') : 'added') as MediaSort);
  let yearFilter = $derived(parseInt($page.url.searchParams.get('year') ?? '') || null);
  let genreFilter = $derived($page.url.searchParams.get('genre') || null);
  let minRating = $derived(parseFloat($page.url.searchParams.get('rating') ?? '') || null);
  let filtered = $derived(!!(yearFilter || genreFilter || minRating));

  const SORTS: { value: MediaSort; label: string }[] = [
    { value: 'added', label: 'Newest' },
    { value: 'rating', label: 'Top rated' },
    { value: 'year', label: 'Release year' },
    { value: 'name', label: 'A–Z' },
  ];

  let groups = $state<ChannelGroup[]>([]);
  let items = $state<Tile[]>([]);
  let facets = $state<MediaFacets>({ years: [], genres: [], rated: 0 });
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let results = $state<Channel[]>([]);
  let searching = $state(false);
  let query = $state($page.url.searchParams.get('q') ?? '');
  let scrolled = $state(false);
  let failed = $state<Set<number>>(new Set());

  let searchEl = $state<HTMLInputElement | undefined>();
  let sentinel = $state<HTMLDivElement | undefined>();
  let headH = $state(0);
  let catFilter = $state('');
  let railEl = $state<HTMLElement | undefined>();

  let listToken = 0;
  let searchToken = 0;
  let debounce: ReturnType<typeof setTimeout> | undefined;

  let isRecent = $derived(group === RECENTLY_ADDED_GROUP);
  let noun = $derived(kind === 'vod' ? 'movies' : 'series');
  let Noun = $derived(kind === 'vod' ? 'Movies' : 'Series');
  let heading = $derived(
    !isRecent
      ? group
      : sort === 'rating'
        ? `Top rated ${noun}`
        : sort === 'year'
          ? `${Noun} by year`
          : sort === 'name'
            ? `${Noun} A–Z`
            : `New ${noun}`,
  );
  let groupCount = $derived(groups.find((g) => g.name === group)?.count ?? null);
  let typeTotal = $derived(groups.reduce((n, g) => n + g.count, 0));
  let sortedGroups = $derived([...groups].filter((g) => g.name).sort((a, b) => b.count - a.count));
  let railGroups = $derived(
    catFilter.trim()
      ? sortedGroups.filter((g) => g.name.toLowerCase().includes(catFilter.trim().toLowerCase()))
      : sortedGroups,
  );

  // Keep the selected category visible in the side panel
  $effect(() => {
    group;
    groups.length;
    queueMicrotask(() => railEl?.querySelector<HTMLElement>('.cat.on')?.scrollIntoView({ block: 'nearest' }));
  });
  let shown = $derived<Tile[]>(q ? results : items);
  let ambient = $derived(shown.find((c) => c.logo_url)?.logo_url ?? null);

  const nf = new Intl.NumberFormat('en-US');

  onMount(() => {
    if (!$playlists.length) loadPlaylists();
    const main = document.querySelector('main');
    const onScroll = () => (scrolled = (main?.scrollTop ?? 0) > 60);
    main?.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      main?.removeEventListener('scroll', onScroll);
      clearTimeout(debounce);
    };
  });

  // Keep the box in sync when the URL changes from outside (back/forward)
  $effect(() => {
    const term = q;
    if (document.activeElement !== searchEl && term !== query.trim()) query = term;
  });

  // Category list for the current playlist + type
  $effect(() => {
    const p = pid;
    const k = kind;
    if (!p) return;
    getGroupsByType(p, k).then((g) => (groups = g)).catch(() => (groups = []));
  });

  // Main list (re-runs when category, sort or filters change)
  $effect(() => {
    if (!pid) return;
    [kind, group, sort, yearFilter, genreFilter, minRating];
    loadList(true);
  });

  // Year / genre options for the current category
  $effect(() => {
    const p = pid;
    const k = kind;
    const g = isRecent ? null : group;
    if (!p) return;
    getMediaFacets(p, k, g).then((f) => (facets = f)).catch(() => {});
  });

  // Search across the whole type
  $effect(() => {
    const p = pid;
    const k = kind;
    const term = q;
    if (!p || !term) {
      results = [];
      searching = false;
      return;
    }
    const token = ++searchToken;
    searching = true;
    searchChannelsInPlaylist(p, k, term, 300)
      .then((r) => token === searchToken && (results = r))
      .catch(() => token === searchToken && (results = []))
      .finally(() => token === searchToken && (searching = false));
  });

  // Infinite scroll
  $effect(() => {
    const el = sentinel;
    if (!el) return;
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting) && hasMore && !loadingMore && !loading && !q) {
        loadList(false);
      }
    }, { rootMargin: '800px 0px' });
    io.observe(el);
    return () => io.disconnect();
  });

  async function loadList(reset: boolean) {
    const p = pid;
    const k = kind;
    const query = {
      group: isRecent ? null : group,
      sort,
      year: yearFilter,
      genre: genreFilter,
      minRating,
    };
    const key = `${p}|${k}|${JSON.stringify(query)}|${$syncState.version}`;
    const token = reset ? ++listToken : listToken;
    const offset = reset ? 0 : items.length;
    if (reset && listCache?.key === key) {
      items = listCache.items;
      hasMore = listCache.hasMore;
      loading = false;
      return;
    }
    if (reset) {
      loading = true;
      items = [];
      hasMore = false;
    } else {
      loadingMore = true;
    }
    try {
      const batch = await browseMedia(p, k, query, PAGE, offset);
      if (token !== listToken) return;
      items = reset ? batch : [...items, ...batch];
      hasMore = batch.length === PAGE;
      listCache = { key, items, hasMore };
    } catch {
      if (token === listToken && reset) items = [];
    } finally {
      if (token === listToken) {
        loading = false;
        loadingMore = false;
      }
    }
  }

  function setParams(next: Record<string, string | null>, replace = false) {
    const url = new URL($page.url);
    for (const [k, v] of Object.entries(next)) {
      if (v === null || v === '') url.searchParams.delete(k);
      else url.searchParams.set(k, v);
    }
    goto(`${url.pathname}${url.search}`, { replaceState: replace, keepFocus: true, noScroll: true });
  }

  function onInput() {
    clearTimeout(debounce);
    debounce = setTimeout(() => setParams({ q: query.trim() || null }, true), 220);
  }

  function clearSearch() {
    query = '';
    clearTimeout(debounce);
    setParams({ q: null }, true);
    searchEl?.focus();
  }

  function pickGroup(g: string) {
    query = '';
    setParams({ group: g === RECENTLY_ADDED_GROUP ? null : g, q: null });
  }

  function pickKind(k: Kind) {
    if (k === kind) return;
    query = '';
    setParams({ type: k, group: null, q: null, year: null, genre: null });
  }

  function clearFilters() {
    setParams({ year: null, genre: null, rating: null });
  }

  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if ((e.key === 'k' && (e.ctrlKey || e.metaKey)) || (e.key === '/' && !typing)) {
      e.preventDefault();
      searchEl?.focus();
      searchEl?.select();
    } else if (e.key === 'Escape' && e.target === searchEl && query) {
      e.preventDefault();
      clearSearch();
    }
  }


  function back() {
    if (history.length > 1) history.back();
    else goto('/');
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="browse">
  {#if ambient}
    {#key ambient}
      <div class="ambient" aria-hidden="true" style:background-image="url('{ambient}')"></div>
    {/key}
  {/if}

  <header class="head" class:scrolled bind:clientHeight={headH}>
    <div class="head-row">
      <button class="back" onclick={back} aria-label="Back">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
      </button>

      <div class="titles">
        <h1 dir="auto">{q ? `“${q}”` : heading}</h1>
        <p>
          {#if q}
            {searching ? 'Searching…' : `${nf.format(results.length)}${results.length >= 300 ? '+' : ''} ${results.length === 1 ? 'result' : 'results'} in ${noun}`}
          {:else if isRecent}
            {sort === 'added' ? 'Latest additions · ' : ''}{nf.format(typeTotal)} {noun} in your library
          {:else if groupCount !== null}
            {nf.format(groupCount)} {groupCount === 1 ? 'title' : 'titles'}
          {/if}
        </p>
      </div>

      <div class="kind" role="tablist" aria-label="Content type">
        <button role="tab" aria-selected={kind === 'vod'} class:on={kind === 'vod'} onclick={() => pickKind('vod')}>Movies</button>
        <button role="tab" aria-selected={kind === 'series'} class:on={kind === 'series'} onclick={() => pickKind('series')}>Series</button>
      </div>

      <label class="search" class:active={!!query}>
        <svg class="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
        <span class="sr-only">Search {noun}</span>
        <input
          bind:this={searchEl}
          bind:value={query}
          oninput={onInput}
          type="search"
          placeholder={`Search all ${noun}`}
          autocomplete="off"
          spellcheck="false"
          dir="auto"
        />
        {#if query}
          <button class="clear" onclick={clearSearch} aria-label="Clear search">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        {:else}
          <kbd>Ctrl K</kbd>
        {/if}
      </label>
    </div>

  </header>

  <div class="layout">
  <aside class="rail" bind:this={railEl} style:top="{headH - 24}px" style:max-height="calc(100vh - {headH}px)" aria-label="Categories">
    <label class="cat-search">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
      <input bind:value={catFilter} placeholder="Find a category" aria-label="Find a category" dir="auto" />
    </label>

    <button class="cat new" class:on={isRecent && !q} aria-current={isRecent && !q ? 'true' : undefined} onclick={() => pickGroup(RECENTLY_ADDED_GROUP)}>
      <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2l2.4 6.9H22l-6 4.6 2.3 7L12 16.3 5.7 20.5 8 13.5 2 8.9h7.6z"/></svg>
      <span class="cat-name">All {noun}</span>
    </button>

    <p class="rail-label">Categories <span>{groups.length}</span></p>
    {#each railGroups as g (g.name)}
      <button class="cat" class:on={group === g.name && !q} aria-current={group === g.name && !q ? 'true' : undefined} onclick={() => pickGroup(g.name)} title={g.name}>
        <span class="cat-name" dir="auto">{g.name}</span>
        <span class="cat-count">{nf.format(g.count)}</span>
      </button>
    {:else}
      <p class="rail-empty">No category matches “{catFilter.trim()}”</p>
    {/each}
  </aside>

  <section class="content">
    {#if !q}
      <div class="toolbar" role="group" aria-label="Sort and filter">
        <label class="select">
          <span>Sort</span>
          <select value={sort} onchange={(e) => setParams({ sort: (e.currentTarget as HTMLSelectElement).value === 'added' ? null : (e.currentTarget as HTMLSelectElement).value })}>
            {#each SORTS as o (o.value)}
              <option value={o.value} disabled={o.value === 'rating' && !facets.rated}>{o.label}</option>
            {/each}
          </select>
        </label>
        {#if facets.years.length}
          <label class="select" class:active={!!yearFilter}>
            <span>Year</span>
            <select value={yearFilter ?? ''} onchange={(e) => setParams({ year: (e.currentTarget as HTMLSelectElement).value || null })}>
              <option value="">Any</option>
              {#each facets.years as y (y.value)}<option value={y.value}>{y.value} ({nf.format(y.count)})</option>{/each}
            </select>
          </label>
        {/if}
        {#if facets.genres.length}
          <label class="select" class:active={!!genreFilter}>
            <span>Genre</span>
            <select value={genreFilter ?? ''} onchange={(e) => setParams({ genre: (e.currentTarget as HTMLSelectElement).value || null })}>
              <option value="">Any</option>
              {#each facets.genres as g (g.value)}<option value={g.value}>{g.value} ({nf.format(g.count)})</option>{/each}
            </select>
          </label>
        {/if}
        {#if facets.rated}
          <button class="toggle" class:on={!!minRating} aria-pressed={!!minRating} onclick={() => setParams({ rating: minRating ? null : '7' })}>
            <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7L12 17.3 5.8 20.9l1.6-7L2 9.2l7.1-.6z"/></svg>
            Rated 7+
          </button>
        {/if}
        {#if filtered}
          <button class="clear-filters" onclick={clearFilters}>Clear filters</button>
        {/if}
      </div>
    {/if}

    {#if (q && searching && !results.length) || (!q && loading)}
      <div class="grid" aria-hidden="true">
        {#each Array(18) as _, i (i)}
          <div class="tile"><div class="art sk"></div><div class="sk sk-line"></div></div>
        {/each}
      </div>
    {:else if !shown.length}
      <div class="empty">
        {#if q}
          <h2>No {noun} match “{q}”</h2>
          <p>Try a shorter name, the original title, or search in {kind === 'vod' ? 'series' : 'movies'} instead.</p>
          <div class="empty-actions">
            <button class="btn" onclick={() => setParams({ type: kind === 'vod' ? 'series' : 'vod', group: null })}>Search in {kind === 'vod' ? 'series' : 'movies'}</button>
            <button class="btn ghost" onclick={clearSearch}>Clear search</button>
          </div>
        {:else}
          <h2>Nothing here yet</h2>
          <p>This category is empty. Refresh your library from the dashboard to pull the latest titles.</p>
        {/if}
      </div>
    {:else}
      <div class="grid">
        {#each shown as c (c.id)}
          {@const t = splitTitle(c.name)}
          {@const year = c.year ?? t.year}
          <a class="tile" href={`/title?id=${c.id}`} aria-label={t.title}>
            <span class="art">
              {#if c.logo_url && !failed.has(c.id)}
                <img src={c.logo_url} alt="" loading="lazy" onerror={() => (failed = new Set(failed).add(c.id))} />
              {:else}
                <span class="art-empty" dir="auto">{t.title}</span>
              {/if}
              {#if c.rating}
                <span class="score" aria-label={`Rated ${c.rating}`}>
                  <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7L12 17.3 5.8 20.9l1.6-7L2 9.2l7.1-.6z"/></svg>
                  {c.rating.toFixed(1)}
                </span>
              {/if}
              <span class="play" aria-hidden="true">
                <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.14v13.72a1 1 0 001.5.86l11-6.86a1 1 0 000-1.72l-11-6.86A1 1 0 008 5.14z"/></svg>
              </span>
            </span>
            <span class="name" dir="auto">{t.title}</span>
            <span class="sub" dir="auto">{year ?? ''}{year && (q || isRecent) ? ' · ' : ''}{q || isRecent ? c.group_name : ''}</span>
          </a>
        {/each}
      </div>

      {#if !q}
        <div class="sentinel" bind:this={sentinel}>
          {#if loadingMore}<span class="spinner" aria-label="Loading more"></span>{/if}
        </div>
      {/if}
    {/if}
  </section>
  </div>
</div>

<style>
  .browse {
    --gutter: clamp(24px, 3.6vw, 60px);
    position: relative;
    margin: -24px;
    min-height: 100vh;
    padding-bottom: 72px;
    isolation: isolate;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }

  .ambient {
    position: absolute;
    inset: 0 0 auto 0;
    height: 520px;
    z-index: -1;
    background-size: cover;
    background-position: center 30%;
    filter: blur(80px) saturate(1.6) brightness(0.5);
    opacity: 0.55;
    mask-image: linear-gradient(to bottom, black, transparent);
    animation: ambientIn 900ms var(--ease-out);
  }
  @keyframes ambientIn { from { opacity: 0; } }

  /* ── Header ─────────────────────────────────────────────────────────── */

  .head {
    position: sticky;
    /* main has 24px padding; stick to the very top of the scroll area */
    top: -24px;
    z-index: 20;
    padding: 20px var(--gutter) 18px;
    transition: background 250ms var(--ease-out), box-shadow 250ms var(--ease-out);
  }
  .head.scrolled {
    background: oklch(0.15 0.004 25 / 0.92);
    backdrop-filter: blur(18px) saturate(1.4);
    -webkit-backdrop-filter: blur(18px) saturate(1.4);
    box-shadow: 0 1px 0 oklch(1 0 0 / 0.06), 0 20px 40px -24px oklch(0 0 0 / 0.8);
  }

  .head-row {
    display: flex;
    align-items: center;
    gap: 18px;
  }

  .back {
    flex-shrink: 0;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(1 0 0 / 0.08);
    color: var(--color-text);
    transition: background 200ms var(--ease-out), transform 200ms var(--ease-out);
  }
  .back :global(svg) { width: 22px; height: 22px; }
  .back:hover { background: oklch(1 0 0 / 0.16); transform: translateX(-2px); }
  .back:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .titles { flex: 1; min-width: 0; }
  .titles h1 {
    font-size: clamp(2rem, 3.6vw, 3.25rem);
    font-weight: 900;
    line-height: 1.02;
    letter-spacing: -0.035em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .titles p {
    margin-top: 4px;
    font-size: 0.9375rem;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .kind {
    flex-shrink: 0;
    display: flex;
    padding: 4px;
    border-radius: 999px;
    background: oklch(1 0 0 / 0.07);
  }
  .kind button {
    height: 34px;
    padding: 0 16px;
    border-radius: 999px;
    background: none;
    color: var(--color-text-muted);
    font-size: 0.875rem;
    font-weight: 700;
    transition: background 200ms var(--ease-out), color 200ms var(--ease-out);
  }
  .kind button:hover { color: var(--color-text); }
  .kind button.on { background: var(--color-text); color: oklch(0.14 0.004 25); }
  .kind button:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .search {
    flex: 0 1 380px;
    min-width: 220px;
    position: relative;
    display: flex;
    align-items: center;
    height: 48px;
    padding: 0 12px 0 46px;
    border-radius: 8px;
    background: oklch(0.12 0.004 25 / 0.7);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.14);
    transition: box-shadow 200ms var(--ease-out), background 200ms var(--ease-out);
    cursor: text;
  }
  .search:hover { box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.28); }
  .search:focus-within {
    background: oklch(0.1 0.004 25 / 0.9);
    box-shadow: inset 0 0 0 2px var(--color-text);
  }
  .search-icon {
    position: absolute;
    left: 15px;
    width: 20px;
    height: 20px;
    color: var(--color-text-muted);
    pointer-events: none;
  }
  .search:focus-within .search-icon,
  .search.active .search-icon { color: var(--color-text); }

  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    background: none;
    font-size: 1rem;
    font-weight: 500;
    color: var(--color-text);
  }
  .search input:focus { border: none; }
  .search input::-webkit-search-cancel-button { display: none; }

  .search kbd {
    flex-shrink: 0;
    padding: 2px 7px;
    border-radius: 4px;
    background: oklch(1 0 0 / 0.08);
    color: var(--color-text-muted);
    font-family: inherit;
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .clear {
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(1 0 0 / 0.1);
    color: var(--color-text);
  }
  .clear :global(svg) { width: 14px; height: 14px; }
  .clear:hover { background: oklch(1 0 0 / 0.2); }

  /* ── Category side panel ───────────────────────────────────────────── */

  .layout {
    display: grid;
    grid-template-columns: clamp(220px, 17vw, 280px) minmax(0, 1fr);
    gap: clamp(16px, 2vw, 32px);
    padding: 0 var(--gutter) 0 calc(var(--gutter) - 12px);
  }

  .rail {
    position: sticky;
    align-self: start;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 4px 32px 0;
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .cat-search {
    position: relative;
    display: flex;
    align-items: center;
    height: 38px;
    margin: 0 4px 10px 12px;
    padding: 0 10px 0 34px;
    border-radius: 8px;
    background: oklch(1 0 0 / 0.06);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.08);
    cursor: text;
    transition: box-shadow 150ms var(--ease-out);
  }
  .cat-search:focus-within { box-shadow: inset 0 0 0 2px var(--color-text); }
  .cat-search :global(svg) { position: absolute; left: 11px; width: 15px; height: 15px; color: var(--color-text-muted); }
  .cat-search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    background: none;
    font-size: 0.8125rem;
  }

  .rail-label {
    display: flex;
    justify-content: space-between;
    margin: 14px 12px 6px;
    font-size: 0.6875rem;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .rail-label span { letter-spacing: 0; font-variant-numeric: tabular-nums; }
  .rail-empty { padding: 8px 12px; font-size: 0.8125rem; color: var(--color-text-muted); }

  .cat {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 0 12px;
    border-radius: 8px;
    background: none;
    color: oklch(0.8 0.004 25);
    font-size: 0.875rem;
    font-weight: 500;
    text-align: start;
    transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
  }
  .cat:hover { background: oklch(1 0 0 / 0.05); color: var(--color-text); }
  .cat:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .cat.on { background: oklch(1 0 0 / 0.1); color: var(--color-text); font-weight: 700; }
  .cat.new { font-weight: 700; color: var(--color-text); }
  .cat.new :global(svg) { width: 16px; height: 16px; flex-shrink: 0; color: var(--color-accent); }
  .cat-name { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cat-count {
    flex-shrink: 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  /* ── Sort & filters ─────────────────────────────────────────────────── */

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 20px;
  }
  .select {
    position: relative;
    display: inline-flex;
    align-items: center;
    height: 36px;
    border-radius: 18px;
    background: oklch(1 0 0 / 0.07);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.1);
    transition: box-shadow 150ms var(--ease-out), background 150ms var(--ease-out);
  }
  .select:hover { background: oklch(1 0 0 / 0.11); }
  .select:focus-within { box-shadow: inset 0 0 0 2px var(--color-text); }
  .select.active { background: oklch(1 0 0 / 0.16); box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.35); }
  .select span {
    padding-left: 14px;
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--color-text-muted);
    pointer-events: none;
  }
  .select select {
    appearance: none;
    height: 100%;
    padding: 0 30px 0 8px;
    border: none;
    border-radius: 18px;
    background:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23ddd' stroke-width='2.5'%3E%3Cpolyline points='6 9 12 15 18 9'/%3E%3C/svg%3E") no-repeat right 12px center / 11px,
      transparent;
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
    cursor: pointer;
  }
  .select select:focus { border: none; }
  .select option { background: oklch(0.2 0.005 25); }

  .toggle, .clear-filters {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 14px;
    border-radius: 18px;
    font-size: 0.8125rem;
    font-weight: 700;
    transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
  }
  .toggle {
    background: oklch(1 0 0 / 0.07);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.1);
    color: oklch(0.88 0.004 25);
  }
  .toggle :global(svg) { width: 14px; height: 14px; color: var(--color-accent-yellow); }
  .toggle:hover { background: oklch(1 0 0 / 0.12); }
  .toggle.on { background: var(--color-text); color: oklch(0.14 0.004 25); box-shadow: none; }
  .clear-filters { background: none; color: var(--color-text-muted); }
  .clear-filters:hover { color: var(--color-text); }
  .toggle:focus-visible, .clear-filters:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .score {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 7px;
    border-radius: 4px;
    background: oklch(0.12 0.004 25 / 0.82);
    backdrop-filter: blur(6px);
    color: var(--color-text);
    font-size: 0.75rem;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }
  .score :global(svg) { width: 11px; height: 11px; color: var(--color-accent-yellow); }

  /* ── Grid ───────────────────────────────────────────────────────────── */

  .content { min-width: 0; padding-top: 18px; }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(clamp(140px, 11vw, 190px), 1fr));
    gap: 30px 14px;
  }

  .tile {
    display: flex;
    flex-direction: column;
    gap: 9px;
    min-width: 0;
    color: var(--color-text);
    text-decoration: none;
    border-radius: 6px;
  }
  .tile:focus-visible { outline: none; }
  .tile:focus-visible .art { outline: 2px solid var(--color-text); outline-offset: 3px; }

  .art {
    position: relative;
    display: block;
    aspect-ratio: 2 / 3;
    border-radius: 6px;
    overflow: hidden;
    background: var(--color-card);
    transition: transform 260ms var(--ease-out), box-shadow 260ms var(--ease-out);
  }
  .art img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .tile:hover .art {
    transform: translateY(-6px) scale(1.04);
    box-shadow: 0 24px 48px -16px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(1 0 0 / 0.12);
  }

  .art-empty {
    display: flex;
    align-items: flex-end;
    width: 100%;
    height: 100%;
    padding: 14px;
    font-size: 1rem;
    font-weight: 800;
    line-height: 1.15;
    background: radial-gradient(120% 80% at 100% 0%, oklch(0.42 0.16 27 / 0.55), transparent 60%), var(--color-card);
  }

  .play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: linear-gradient(to top, oklch(0 0 0 / 0.55), transparent 55%);
    opacity: 0;
    transition: opacity 200ms var(--ease-out);
  }
  .play :global(svg) {
    width: 46px;
    height: 46px;
    padding: 12px 11px 12px 13px;
    border-radius: 50%;
    background: oklch(0.98 0.004 25 / 0.95);
    color: oklch(0.14 0.004 25);
    transform: scale(0.85);
    transition: transform 260ms var(--ease-out);
  }
  .tile:hover .play,
  .tile:focus-visible .play { opacity: 1; }
  .tile:hover .play :global(svg) { transform: scale(1); }

  .name {
    font-size: 0.875rem;
    font-weight: 700;
    line-height: 1.25;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    margin-top: -5px;
    font-size: 0.75rem;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sentinel {
    display: grid;
    place-items: center;
    height: 96px;
  }
  .spinner {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 3px solid oklch(1 0 0 / 0.12);
    border-top-color: var(--color-accent);
    animation: spin 800ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .sk {
    background: linear-gradient(90deg, oklch(1 0 0 / 0.05) 0%, oklch(1 0 0 / 0.1) 50%, oklch(1 0 0 / 0.05) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.6s linear infinite;
  }
  .sk-line { height: 13px; width: 75%; border-radius: 4px; }
  @keyframes shimmer {
    from { background-position: 100% 0; }
    to { background-position: -100% 0; }
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 72px 0;
    max-width: 560px;
  }
  .empty h2 { font-size: 1.75rem; font-weight: 800; letter-spacing: -0.02em; }
  .empty p { color: var(--color-text-muted); line-height: 1.5; }
  .empty-actions { display: flex; gap: 10px; margin-top: 8px; }
  .btn {
    height: 42px;
    padding: 0 20px;
    border-radius: 6px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-weight: 700;
  }
  .btn:hover { background: oklch(0.85 0.004 25); }
  .btn.ghost { background: oklch(1 0 0 / 0.1); color: var(--color-text); }
  .btn.ghost:hover { background: oklch(1 0 0 / 0.18); }

  @media (max-width: 1000px) {
    .head-row { flex-wrap: wrap; }
    .search { flex-basis: 100%; order: 4; }
  }

  @media (prefers-reduced-motion: reduce) {
    .sk, .spinner, .ambient { animation: none; }
    .art, .play, .play :global(svg) { transition: none; }
  }
</style>
