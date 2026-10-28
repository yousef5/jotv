<script lang="ts" module>
  import type { DashboardStats, ContentTypeCount, Channel, FavoriteChannel, SeriesUpdate } from '$lib/tauri';

  interface GroupRow { title: string; items: Channel[] }
  interface GenreRow { title: string; genre: string; pid: number; items: Channel[] }

  interface HomeData {
    stats: DashboardStats | null;
    countsMap: Record<number, ContentTypeCount[]>;
    recentChannels: Channel[];
    newVod: Channel[];
    newSeries: Channel[];
    groupRows: GroupRow[];
    genreRows: GenreRow[];
    seriesUpdates: SeriesUpdate[];
    topRated: Channel[];
    favorites: FavoriteChannel[];
    heroIndex: number;
  }

  // Last rendered dashboard: going back shows it instantly, so the scroll
  // position can be restored before the background refresh finishes.
  let homeCache: HomeData | null = null;
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import AddPlaylistModal from '$lib/components/AddPlaylistModal.svelte';
  import SystemCheck from '$lib/components/SystemCheck.svelte';
  import logoMark from '$lib/assets/logo-mark.png';
  import Hero from '$lib/components/Hero.svelte';
  import MediaRow from '$lib/components/MediaRow.svelte';
  import type { RowItem } from '$lib/components/MediaRow.svelte';
  import SyncButton from '$lib/components/SyncButton.svelte';
  import { splitTitle } from '$lib/playback';
  import {
    getDashboardStats, getContentTypeCounts, getRecentlyWatched,
    getFavorites, getRecentlyAdded, getChannelsByGroup, browseMedia,
    getSeriesUpdates, getMediaFacets,
  } from '$lib/tauri';
  import { agoLabel } from '$lib/mediaInfo';
  import type { Playlist } from '$lib/tauri';
  import { playlists, loadPlaylists, pendingOpen, RECENTLY_ADDED_GROUP } from '$lib/stores/playlists';
  import type { PendingOpen } from '$lib/stores/playlists';
  import { syncState } from '$lib/stores/sync';
  import { accounts, accountAlerts, loadAccount } from '$lib/stores/account';
  import { openSearch } from '$lib/stores/search';
  import { mainScrollSnapshot } from '$lib/scroll';

  export const snapshot = mainScrollSnapshot;

  const FEATURED_COUNT = 6;
  // Too broad to make an interesting row
  const BROAD_GENRES = ['drama', 'kids', 'family', 'tv movie'];

  const cached = homeCache;

  let stats: DashboardStats | null = $state(cached?.stats ?? null);
  let loaded = $state(!!cached);
  let contentLoading = $state(!cached);
  let showAddModal = $state(false);
  let alertDismissed = $state(false);
  let countsMap = $state<Record<number, ContentTypeCount[]>>(cached?.countsMap ?? {});
  let recentChannels = $state<Channel[]>(cached?.recentChannels ?? []);
  let newVod = $state<Channel[]>(cached?.newVod ?? []);
  let newSeries = $state<Channel[]>(cached?.newSeries ?? []);
  let groupRows = $state<GroupRow[]>(cached?.groupRows ?? []);
  let genreRows = $state<GenreRow[]>(cached?.genreRows ?? []);
  let seriesUpdates = $state<SeriesUpdate[]>(cached?.seriesUpdates ?? []);
  let topRated = $state<Channel[]>(cached?.topRated ?? []);
  let favorites = $state<FavoriteChannel[]>(cached?.favorites ?? []);

  let heroIndex = $state(cached?.heroIndex ?? 0);
  // Alternate movies and series for the billboard
  let featured = $derived.by(() => {
    const vod = newVod.filter((c) => c.logo_url);
    const series = newSeries.filter((c) => c.logo_url);
    const out: Channel[] = [];
    for (let i = 0; out.length < FEATURED_COUNT && (i < vod.length || i < series.length); i++) {
      if (vod[i]) out.push(vod[i]);
      if (series[i] && out.length < FEATURED_COUNT) out.push(series[i]);
    }
    return out;
  });

  onMount(() => {
    loadContent();
    return () => {
      homeCache = $state.snapshot({
        stats, countsMap, recentChannels, newVod, newSeries, groupRows, genreRows, seriesUpdates, topRated, favorites, heroIndex,
      }) as HomeData;
    };
  });

  // Reload rows whenever a sync finishes (launch sync or the refresh button)
  let seenVersion = get(syncState).version;
  $effect(() => {
    const v = $syncState.version;
    if (v !== seenVersion) {
      seenVersion = v;
      loadContent();
    }
  });

  async function loadContent() {
    await loadPlaylists();
    loaded = true;
    const list = $playlists;

    getDashboardStats().then((s) => (stats = s)).catch(() => {});
    getRecentlyWatched(16).then((r) => (recentChannels = r)).catch(() => {});
    getFavorites().then((f) => (favorites = f)).catch(() => {});
    getSeriesUpdates(30, 20).then((u) => (seriesUpdates = u)).catch(() => {});
    for (const p of list) loadPlaylistDetails(p);

    let vod: Channel[] = [];
    let series: Channel[] = [];
    let vodPid = 0;
    let seriesPid = 0;
    for (const p of list) {
      if (!vod.length) { vod = await getRecentlyAdded(p.id, 'vod', 30).catch(() => []); vodPid = p.id; }
      if (!series.length) { series = await getRecentlyAdded(p.id, 'series', 30).catch(() => []); seriesPid = p.id; }
      if (vod.length && series.length) break;
    }
    newVod = vod;
    newSeries = series;

    // A few "more like this" rows from the groups that just got new titles
    const picks = [
      ...topGroups(vod, 2).map((g) => ({ pid: vodPid, type: 'vod', group: g })),
      ...topGroups(series, 1).map((g) => ({ pid: seriesPid, type: 'series', group: g })),
    ];
    const rows = await Promise.all(
      picks.map(async ({ pid, type, group }) => ({
        title: group,
        items: await getChannelsByGroup(pid, type, group, 30, 0).catch(() => [] as Channel[]),
      })),
    );
    groupRows = rows.filter((r) => r.items.length >= 4);
    if (seriesPid) genreRows = await loadGenreRows(seriesPid);

    // Best-rated movies released this year (ratings come from the provider's list)
    if (vodPid) {
      const year = new Date().getFullYear();
      topRated = await browseMedia(vodPid, 'vod', { sort: 'rating', year, minRating: 6 }, 30).catch(() => []);
    }
    contentLoading = false;
  }

  function topGroups(items: Channel[], n: number): string[] {
    const counts = new Map<string, number>();
    for (const c of items) if (c.group_name) counts.set(c.group_name, (counts.get(c.group_name) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, n).map(([g]) => g);
  }

  /** Two top-rated series rows by genre; which genres changes every day */
  async function loadGenreRows(pid: number): Promise<GenreRow[]> {
    const facets = await getMediaFacets(pid, 'series', null).catch(() => null);
    // Latin names only, to match the English interface
    const genres = (facets?.genres ?? [])
      .map((g) => g.value)
      .filter((g) => /^[A-Za-z&' -]+$/.test(g) && !BROAD_GENRES.includes(g.toLowerCase()))
      .slice(0, 10);
    if (!genres.length) return [];
    const day = Math.floor(Date.now() / 86_400_000);
    const picks = [...new Set([genres[day % genres.length], genres[(day + 3) % genres.length]])];
    const rows = await Promise.all(
      picks.map(async (genre) => ({
        title: `${genre} series`,
        genre,
        pid,
        items: await browseMedia(pid, 'series', { genre, sort: 'rating', minRating: 7 }, 30).catch(() => [] as Channel[]),
      })),
    );
    return rows.filter((r) => r.items.length >= 4);
  }

  async function loadPlaylistDetails(p: Playlist) {
    getContentTypeCounts(p.id).then((c) => (countsMap = { ...countsMap, [p.id]: c })).catch(() => {});
    loadAccount(p);
  }

  // ── Navigation ──────────────────────────────────────────────────────────────

  /** Live channels play from the Live TV screen */
  function go(req: PendingOpen) {
    pendingOpen.set(req);
    goto('/live');
  }

  /** Open a title: live plays, movies play in the app with 'play', otherwise their detail page */
  function open(ch: Channel, action: 'detail' | 'play' = 'detail') {
    if (ch.content_type === 'live') {
      return go({ playlistId: ch.playlist_id, contentType: 'live', channel: ch, action: 'play' });
    }
    goto(action === 'play' && ch.content_type === 'vod' ? `/watch?id=${ch.id}` : `/title?id=${ch.id}`);
  }

  /** Full-page grid for a row: recently added (default) or one category */
  function browse(sample: Channel | undefined, group?: string) {
    if (!sample || sample.content_type === 'live') return goto('/live');
    const params = new URLSearchParams({ pid: String(sample.playlist_id), type: sample.content_type });
    if (group && group !== RECENTLY_ADDED_GROUP) params.set('group', group);
    goto(`/browse?${params}`);
  }

  function openFavorite(f: FavoriteChannel) {
    open({
      id: f.channel_id,
      playlist_id: f.playlist_id,
      name: f.channel_name,
      group_name: f.group_name,
      stream_url: f.stream_url,
      logo_url: f.logo_url,
      epg_id: null,
      content_type: f.content_type,
      created_at: f.added_at,
    });
  }

  function toItem(c: Channel, sub?: string): RowItem {
    return {
      key: c.id,
      title: splitTitle(c.name).title,
      subtitle: sub ?? splitTitle(c.name).year ?? undefined,
      image: c.logo_url,
      live: c.content_type === 'live',
    };
  }

  let continueItems = $derived(
    recentChannels.map((c) => toItem(c, c.content_type === 'live' ? c.group_name : c.content_type === 'vod' ? 'Movie' : 'Series')),
  );
  let favoriteItems = $derived(
    favorites.slice(0, 24).map((f): RowItem => ({
      key: f.id,
      title: f.channel_name,
      subtitle: f.group_name,
      image: f.logo_url,
      live: f.content_type === 'live',
    })),
  );

  // ── Library summary ─────────────────────────────────────────────────────────

  const nf = new Intl.NumberFormat('en-US');

  function getCount(pid: number, type: string): number {
    return countsMap[pid]?.find((c) => c.content_type === type)?.count ?? 0;
  }

  function daysUntilExpiry(ts: string | null): number | null {
    const n = parseInt(ts ?? '');
    return isNaN(n) ? null : Math.ceil((n - Date.now() / 1000) / 86400);
  }

  function formatExpDate(ts: string | null): string {
    const n = parseInt(ts ?? '');
    if (isNaN(n)) return 'No expiry';
    return new Date(n * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' });
  }

  async function onPlaylistAdded() {
    showAddModal = false;
    await loadContent();
  }
</script>

<div class="home">
  {#if loaded && $playlists.length === 0}
    <!-- First run -->
    <section class="welcome">
      <div class="welcome-glow" aria-hidden="true"></div>
      <div class="welcome-copy">
        <span class="brand"><img src={logoMark} alt="" /><span class="word"><b>Jo</b>Tv</span></span>
        <h1>Your channels, films and series in one place.</h1>
        <p>Add an Xtream account or an M3U link and JoTV builds your library.</p>
        <button class="btn-play" onclick={() => (showAddModal = true)}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
          Add your first playlist
        </button>
        <!-- Linux: are mpv, FFmpeg and yt-dlp installed? (Windows ships them) -->
        <div class="welcome-check"><SystemCheck compact /></div>
      </div>
    </section>
  {:else}
    <!-- Billboard -->
    <Hero items={featured} bind:index={heroIndex} label="New on JoTV">
      {#if $accountAlerts.length && !alertDismissed}
        {@const a = $accountAlerts[0]}
        <div class="alert" class:expired={a.kind === 'expired'} role="status">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10.3 3.9L1.8 18a2 2 0 001.7 3h17a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
          <span>
            {#if a.kind === 'expired'}
              Your <b>{a.name}</b> subscription has expired. Channels and movies won't play until it's renewed.
            {:else}
              Your <b>{a.name}</b> subscription ends in {a.days} {a.days === 1 ? 'day' : 'days'}.
            {/if}
          </span>
          <button onclick={() => goto('/playlists')}>Details</button>
          <button class="x" onclick={() => (alertDismissed = true)} aria-label="Dismiss">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        </div>
      {/if}

      <div class="topbar">
        <button class="search-pill" onclick={() => openSearch()} aria-label="Search everything">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
          <span>Search channels, movies, series</span>
          <kbd>/</kbd>
        </button>
        <SyncButton />
        <button class="pill" onclick={() => (showAddModal = true)}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
          Add playlist
        </button>
      </div>

      {#snippet fallback()}
        {#if contentLoading}
          <div class="hero-skeleton" aria-hidden="true">
            <div class="sk sk-kicker"></div>
            <div class="sk sk-title"></div>
            <div class="sk sk-line"></div>
            <div class="sk sk-line short"></div>
            <div class="sk-actions"><div class="sk sk-btn"></div><div class="sk sk-btn"></div></div>
          </div>
        {:else}
          <div class="hero-empty">
            <h1>Live TV</h1>
            <p>Your playlists only have live channels. Jump in from the guide.</p>
            <button class="btn-play" onclick={() => goto('/live')}>Browse channels</button>
          </div>
        {/if}
      {/snippet}
    </Hero>

    <!-- Rows -->
    <div class="rows">
      {#if continueItems.length}
        <MediaRow
          title="Continue watching"
          variant="landscape"
          items={continueItems}
          onselect={(i) => open(recentChannels[i])}
        />
      {/if}

      {#if seriesUpdates.length}
        <MediaRow
          title="New episodes for you"
          items={seriesUpdates.map((u) => ({ ...toItem(u, `Updated ${agoLabel(u.updated_at)}`), badge: 'New episodes' }))}
          onselect={(i) => open(seriesUpdates[i])}
        />
      {/if}

      {#if newVod.length}
        <MediaRow
          title="New movies"
          items={newVod.map((c) => toItem(c))}
          onselect={(i) => open(newVod[i])}
          onviewall={() => browse(newVod[0], RECENTLY_ADDED_GROUP)}
        />
      {/if}

      {#if topRated.length >= 4}
        <MediaRow
          title={`Top rated ${new Date().getFullYear()}`}
          items={topRated.map((c) => toItem(c))}
          onselect={(i) => open(topRated[i])}
          onviewall={() => goto(`/browse?pid=${topRated[0].playlist_id}&type=vod&sort=rating&year=${new Date().getFullYear()}`)}
        />
      {/if}

      {#if favoriteItems.length}
        <MediaRow
          title="My list"
          variant="landscape"
          items={favoriteItems}
          onselect={(i) => openFavorite(favorites[i])}
          onviewall={() => goto('/favorites')}
        />
      {/if}

      {#if newSeries.length}
        <MediaRow
          title="New series & episodes"
          items={newSeries.map((c) => toItem(c))}
          onselect={(i) => open(newSeries[i])}
          onviewall={() => browse(newSeries[0], RECENTLY_ADDED_GROUP)}
        />
      {/if}

      {#each groupRows as row (row.title)}
        <MediaRow
          title={row.title}
          items={row.items.map((c) => toItem(c))}
          onselect={(i) => open(row.items[i])}
          onviewall={() => browse(row.items[0], row.title)}
        />
      {/each}

      {#each genreRows as row (row.title)}
        <MediaRow
          title={row.title}
          items={row.items.map((c) => toItem(c))}
          onselect={(i) => open(row.items[i])}
          onviewall={() => goto(`/browse?${new URLSearchParams({ pid: String(row.pid), type: 'series', genre: row.genre, sort: 'rating' })}`)}
        />
      {/each}

      {#if contentLoading && !newVod.length}
        {#each [0, 1] as r (r)}
          <div class="row-skeleton" aria-hidden="true">
            <div class="sk sk-row-title"></div>
            <div class="sk-tiles">
              {#each Array(8) as _, i (i)}<div class="sk sk-tile"></div>{/each}
            </div>
          </div>
        {/each}
      {/if}

      <!-- Library -->
      <section class="library">
        <header class="library-head">
          <h2>Your library</h2>
          {#if stats}
            <p>
              {nf.format(stats.total_channels)} titles
              <span aria-hidden="true">·</span> {nf.format(stats.total_favorites)} in my list
              <span aria-hidden="true">·</span> {nf.format(stats.total_downloads)} downloads
            </p>
          {/if}
        </header>

        <ul class="accounts">
          {#each $playlists as p (p.id)}
            {@const info = $accounts[p.id]}
            {@const days = info ? daysUntilExpiry(info.exp_date) : null}
            <li>
              <button class="account" onclick={() => goto('/playlists')}>
                <span class="acc-name">
                  <span class="acc-type">{p.source_type === 'xtream' ? 'Xtream' : 'M3U'}</span>
                  <strong dir="auto">{p.name}</strong>
                </span>
                <span class="acc-counts">
                  <span><b>{nf.format(getCount(p.id, 'live'))}</b> live</span>
                  <span><b>{nf.format(getCount(p.id, 'vod'))}</b> movies</span>
                  <span><b>{nf.format(getCount(p.id, 'series'))}</b> series</span>
                </span>
                {#if info}
                  <span class="acc-status">
                    <i class="status-dot" class:active={info.status === 'Active'}></i>
                    {info.status}
                    <span class="sep">·</span>
                    {info.active_cons ?? '0'}/{info.max_connections ?? '?'} streams
                  </span>
                  <span
                    class="acc-expiry"
                    class:warn={days !== null && days <= 7 && days > 0}
                    class:expired={days !== null && days <= 0}
                  >
                    {#if days !== null && days <= 0}Expired{:else if days !== null}{days} days left{:else}No expiry{/if}
                    <small>{formatExpDate(info.exp_date)}</small>
                  </span>
                {:else}
                  <span class="acc-status"></span>
                  <span class="acc-expiry"></span>
                {/if}
                <svg class="acc-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 6 15 12 9 18"/></svg>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    </div>
  {/if}
</div>

{#if showAddModal}
  <AddPlaylistModal onclose={onPlaylistAdded} />
{/if}

<style>
  .home {
    --gutter: clamp(24px, 3.6vw, 60px);
    margin: -24px;
    min-height: 100vh;
    padding-bottom: 56px;
  }

  /* ── Shared bits ─────────────────────────────────────────────────────── */

  
  
  

  .btn-play {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 58px;
    min-width: 230px;
    padding: 0 34px 0 28px;
    border-radius: 999px;
    background: var(--color-text);
    color: oklch(var(--n140));
    font-size: 1.125rem;
    font-weight: 800;
    box-shadow: 0 12px 30px -10px oklch(0 0 0 / 0.6);
    transition: background 200ms var(--ease-out);
  }
  .btn-play :global(svg) { width: 24px; height: 24px; }
  .btn-play:hover { background: oklch(var(--n860)); }

  
  
  
  

  .btn-play:active { transform: scale(0.96); }
  .btn-play:focus-visible { outline: 2px solid var(--color-text); outline-offset: 3px; }

  /* ── Billboard ───────────────────────────────────────────────────────── */

  

  .topbar {
    position: absolute;
    top: 20px;
    right: var(--gutter);
    z-index: 10;
    display: flex;
    gap: 10px;
  }

  .alert {
    position: absolute;
    top: 20px;
    left: var(--gutter);
    z-index: 11;
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: min(640px, calc(100% - 380px));
    padding: 8px 8px 8px 14px;
    border-radius: 10px;
    background: oklch(0.78 0.15 75 / 0.16);
    box-shadow: inset 0 0 0 1px oklch(0.78 0.15 75 / 0.45);
    color: var(--color-text);
    font-size: 0.875rem;
  }
  .alert > :global(svg) { width: 18px; height: 18px; flex-shrink: 0; color: var(--color-accent-yellow); }
  .alert.expired { background: oklch(0.58 0.225 27 / 0.2); box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.5); }
  .alert.expired > :global(svg) { color: var(--color-accent-soft); }
  .alert span { flex: 1; min-width: 0; }
  .alert button {
    flex-shrink: 0;
    height: 30px;
    padding: 0 12px;
    border-radius: 6px;
    background: oklch(var(--n1000) / 0.12);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
  }
  .alert button:hover { background: oklch(var(--n1000) / 0.2); }
  .alert button.x { width: 30px; padding: 0; display: grid; place-items: center; background: none; }
  .alert button.x :global(svg) { width: 14px; height: 14px; }

  .search-pill {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    width: clamp(220px, 24vw, 340px);
    height: 36px;
    padding: 0 8px 0 13px;
    border-radius: 999px;
    background: oklch(var(--n130) / 0.8);
    box-shadow: inset 0 0 0 1px oklch(var(--n1000) / 0.14);
    color: oklch(var(--n780));
    font-size: 0.8125rem;
    font-weight: 600;
    text-align: start;
    transition: background 200ms var(--ease-out), box-shadow 200ms var(--ease-out);
  }
  .search-pill :global(svg) { width: 17px; height: 17px; flex-shrink: 0; }
  .search-pill span { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .search-pill kbd {
    padding: 1px 7px;
    border-radius: 4px;
    background: oklch(var(--n1000) / 0.1);
    font-family: inherit;
    font-size: 0.6875rem;
    font-weight: 700;
  }
  .search-pill:hover { background: oklch(var(--n220) / 0.75); box-shadow: inset 0 0 0 1px oklch(var(--n1000) / 0.28); color: var(--color-text); }
  .search-pill:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 16px 0 12px;
    border-radius: 999px;
    background: var(--color-accent);
    color: var(--color-on-accent);
    font-size: 0.8125rem;
    font-weight: 700;
    transition: background 200ms var(--ease-out);
  }
  .pill :global(svg) { width: 16px; height: 16px; }
  .pill:hover { background: var(--color-accent-hover); }
  .pill:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  

  

  

  /* No wide backdrop: show the poster sharp at a size it holds up at, over its own glow */
  

  

  

  

  

  
  

  
  
  
  

  
  
  
  

  

  
  
  
  
  /* Grows with transform only, so its neighbours never shift */
  
  /* WebKitGTK draws a square native ring on click; use a round one for keyboard focus only */
  
  

  /* Slide timer: a hairline across the picker */
  
  
  
  

  

  

  .hero-empty {
    position: absolute;
    left: var(--gutter);
    bottom: 84px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
  }
  .hero-empty h1 { font-size: clamp(2.5rem, 5vw, 4.75rem); font-weight: 900; letter-spacing: -0.035em; }
  .hero-empty p { font-size: 1.0625rem; color: var(--color-text-muted); }

  /* ── Rows ────────────────────────────────────────────────────────────── */

  .rows {
    position: relative;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  

  /* ── Library ─────────────────────────────────────────────────────────── */

  .library {
    margin: 28px var(--gutter) 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .library-head {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 6px 16px;
  }
  .library-head h2 {
    font-size: 1.25rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .library-head p {
    font-size: 0.875rem;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .accounts {
    list-style: none;
    border-top: 1px solid oklch(var(--n1000) / 0.08);
  }
  .accounts li { border-bottom: 1px solid oklch(var(--n1000) / 0.08); }

  .account {
    width: 100%;
    display: grid;
    grid-template-columns: minmax(180px, 1.3fr) minmax(240px, 1.6fr) minmax(160px, 1fr) minmax(140px, 0.9fr) 20px;
    align-items: center;
    gap: 20px;
    padding: 16px 12px;
    margin: 0 -12px;
    width: calc(100% + 24px);
    border-radius: 6px;
    background: transparent;
    color: var(--color-text);
    text-align: start;
    font-size: 0.875rem;
    transition: background 200ms var(--ease-out);
  }
  .account:hover { background: oklch(var(--n1000) / 0.04); }
  .account:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }

  .acc-name {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .acc-name strong {
    font-size: 0.9375rem;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acc-type {
    flex-shrink: 0;
    padding: 2px 6px;
    border-radius: 3px;
    background: oklch(var(--n1000) / 0.08);
    color: var(--color-text-muted);
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .acc-counts {
    display: flex;
    gap: 18px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .acc-counts b { color: var(--color-text); font-weight: 700; }

  .acc-status {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--color-text-muted);
  }
  .acc-status .sep { opacity: 0.5; }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
  }
  .status-dot.active {
    background: var(--color-accent-green);
    box-shadow: 0 0 0 3px oklch(0.76 0.13 165 / 0.18);
  }

  .acc-expiry {
    display: flex;
    flex-direction: column;
    font-weight: 600;
    color: var(--color-text);
  }
  .acc-expiry small {
    font-size: 0.75rem;
    font-weight: 400;
    color: var(--color-text-muted);
  }
  .acc-expiry.warn { color: var(--color-accent-yellow); }
  .acc-expiry.expired { color: var(--color-accent-soft); }

  .acc-chevron {
    width: 18px;
    height: 18px;
    color: var(--color-text-muted);
    transition: color 200ms var(--ease-out);
  }
  .account:hover .acc-chevron { color: var(--color-text); }

  @media (max-width: 1100px) {
    .account { grid-template-columns: 1fr auto 20px; }
    .acc-counts, .acc-status { display: none; }
  }

  /* ── Loading ─────────────────────────────────────────────────────────── */

  .sk {
    border-radius: 4px;
    background: linear-gradient(90deg, oklch(var(--n1000) / 0.05) 0%, oklch(var(--n1000) / 0.1) 50%, oklch(var(--n1000) / 0.05) 100%);
    background-size: 200% 100%;
    /* Hidden for the first 160ms: fast loads go straight to content instead of
       flashing a skeleton for a frame or two */
    visibility: hidden;
    animation: shimmer 1.6s linear infinite, sk-show 0s 160ms forwards;
  }
  @keyframes sk-show { to { visibility: visible; } }

  .hero-skeleton {
    position: absolute;
    left: var(--gutter);
    bottom: 84px;
    width: min(560px, 46vw);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .sk-kicker { width: 90px; height: 18px; }
  .sk-title { width: 80%; height: 64px; }
  .sk-line { width: 100%; height: 14px; }
  .sk-line.short { width: 65%; }
  .sk-actions { display: flex; gap: 12px; margin-top: 8px; }
  .sk-btn { width: 130px; height: 46px; border-radius: 6px; }

  .row-skeleton {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 0 var(--gutter);
    overflow: hidden;
  }
  .sk-row-title { width: 160px; height: 20px; }
  .sk-tiles { display: flex; gap: 8px; }
  .sk-tile { flex: 0 0 clamp(132px, 10.5vw, 188px); aspect-ratio: 2 / 3; border-radius: 6px; }

  @keyframes shimmer {
    from { background-position: 100% 0; }
    to { background-position: -100% 0; }
  }

  /* ── First run ───────────────────────────────────────────────────────── */

  .welcome {
    position: relative;
    min-height: 100vh;
    display: flex;
    align-items: center;
    padding: 0 var(--gutter);
    overflow: hidden;
  }
  .welcome-glow {
    position: absolute;
    inset: -20% -10% auto auto;
    width: 70vw;
    height: 90vh;
    background: radial-gradient(closest-side, oklch(0.45 0.18 27 / 0.45), transparent);
    filter: blur(40px);
  }
  .welcome-copy {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 18px;
    max-width: 640px;
  }
  .brand { display: inline-flex; align-items: center; gap: 12px; }
  .brand img { width: 64px; height: 64px; object-fit: contain; }
  .brand .word { font-size: 2rem; font-weight: 900; letter-spacing: -0.04em; color: var(--color-text); }
  .brand .word b { font-weight: 900; color: var(--color-accent); }
  .welcome-check { width: 100%; margin-top: 18px; }
  .welcome h1 {
    font-size: clamp(2.5rem, 5vw, 4.25rem);
    font-weight: 900;
    line-height: 1;
    letter-spacing: -0.035em;
    text-wrap: balance;
  }
  .welcome p {
    font-size: 1.125rem;
    color: var(--color-text-muted);
    max-width: 48ch;
  }

  @media (prefers-reduced-motion: reduce) {
    .sk { animation: none; visibility: visible; }
    /* No auto-advance: without an animation, animationend never fires */
  }
</style>
