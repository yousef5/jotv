<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import StatCard from '$lib/components/StatCard.svelte';
  import AddPlaylistModal from '$lib/components/AddPlaylistModal.svelte';
  import { getDashboardStats, getXtreamAccountInfo, getContentTypeCounts, getRecentlyWatched, getFavorites, getRecentlyAdded } from '$lib/tauri';
  import type { DashboardStats, Playlist, XtreamAccountInfo, ContentTypeCount, Channel, FavoriteChannel } from '$lib/tauri';
  import { playlists, loadPlaylists } from '$lib/stores/playlists';

  let stats: DashboardStats | null = $state(null);
  let loaded = $state(false);
  let showAddModal = $state(false);
  let accountInfoMap = $state<Record<number, XtreamAccountInfo | null>>({});
  let countsMap = $state<Record<number, ContentTypeCount[]>>({});
  let recentChannels = $state<Channel[]>([]);
  let newVod = $state<Channel[]>([]);
  let newSeries = $state<Channel[]>([]);
  let recentFavorites = $state<FavoriteChannel[]>([]);

  onMount(async () => {
    await loadPlaylists();
    loaded = true;
    try { stats = await getDashboardStats(); } catch {}
    try { recentChannels = await getRecentlyWatched(12); } catch {}
    try { recentFavorites = await getFavorites(); } catch {}
    for (const p of $playlists) {
      loadPlaylistDetails(p);
      // Load recently added VOD & series from first playlist
      if (newVod.length === 0) {
        try { newVod = await getRecentlyAdded(p.id, 'vod', 12); } catch {}
      }
      if (newSeries.length === 0) {
        try { newSeries = await getRecentlyAdded(p.id, 'series', 12); } catch {}
      }
    }
  });

  async function loadPlaylistDetails(p: Playlist) {
    try { const c = await getContentTypeCounts(p.id); countsMap = { ...countsMap, [p.id]: c }; } catch {}
    if (p.source_type === 'xtream' && p.source_url && p.xtream_username && p.xtream_password) {
      try {
        const info = await getXtreamAccountInfo(p.source_url, p.xtream_username, p.xtream_password);
        accountInfoMap = { ...accountInfoMap, [p.id]: info };
      } catch { accountInfoMap = { ...accountInfoMap, [p.id]: null }; }
    }
  }

  function formatExpDate(ts: string | null): string {
    if (!ts) return 'N/A';
    const n = parseInt(ts);
    if (isNaN(n)) return 'N/A';
    return new Date(n * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' });
  }

  function daysUntilExpiry(ts: string | null): number | null {
    if (!ts) return null;
    const n = parseInt(ts);
    if (isNaN(n)) return null;
    return Math.ceil((n - Date.now() / 1000) / 86400);
  }

  function getCount(pid: number, type: string): number {
    return countsMap[pid]?.find(c => c.content_type === type)?.count ?? 0;
  }

  function getTotalCount(pid: number): number {
    return countsMap[pid]?.reduce((s, c) => s + c.count, 0) ?? 0;
  }

  function timeAgo(dateStr: string): string {
    const diff = (Date.now() - new Date(dateStr + 'Z').getTime()) / 1000;
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff/60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff/3600)}h ago`;
    if (diff < 604800) return `${Math.floor(diff/86400)}d ago`;
    return new Date(dateStr).toLocaleDateString();
  }

  async function onPlaylistAdded() {
    showAddModal = false;
    await loadPlaylists();
    stats = await getDashboardStats();
    for (const p of $playlists) { if (!countsMap[p.id]) loadPlaylistDetails(p); }
  }
</script>

<div class="dashboard fade-in">
  <!-- Header -->
  <div class="page-header">
    <div>
      <h1 class="page-title">Dashboard</h1>
      <span class="page-subtitle">Welcome back</span>
    </div>
    <button class="btn-accent add-btn" onclick={() => showAddModal = true}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
      Add Playlist
    </button>
  </div>

  {#if loaded && $playlists.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 6h16M4 10h16M4 14h10"/><path d="M18 14v6M15 17h6"/></svg>
      </div>
      <h2>No playlists yet</h2>
      <p>Add your first playlist to get started</p>
      <button class="btn-accent" onclick={() => showAddModal = true}>Add Playlist</button>
    </div>
  {:else}
    <!-- Stats -->
    {#if stats}
      <div class="stats-grid">
        <StatCard label="Channels" value={stats.total_channels} color="var(--color-accent)" icon="channels" />
        <StatCard label="Favorites" value={stats.total_favorites} color="var(--color-accent-green)" icon="favorites" />
        <StatCard label="Playlists" value={stats.total_playlists} color="var(--color-accent-purple)" icon="playlists" />
        <StatCard label="Downloads" value={stats.total_downloads} color="var(--color-accent-yellow)" icon="downloads" />
      </div>
    {/if}

    <div class="dashboard-grid">
      <!-- Recently Watched -->
      <section class="section">
        <div class="section-header">
          <h2 class="section-title">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
            Recently Watched
          </h2>
          {#if recentChannels.length > 0}
            <button class="section-link" onclick={() => goto('/playlists')}>View all</button>
          {/if}
        </div>
        {#if recentChannels.length === 0}
          <div class="section-empty">No watch history yet</div>
        {:else}
          <div class="recent-list">
            {#each recentChannels.slice(0, 8) as ch (ch.id)}
              <button class="recent-item" onclick={() => goto('/playlists')}>
                {#if ch.logo_url}
                  <img class="recent-icon" src={ch.logo_url} alt="" loading="lazy" />
                {:else}
                  <div class="recent-icon placeholder">
                    <span>{ch.name.charAt(0).toUpperCase()}</span>
                  </div>
                {/if}
                <div class="recent-text">
                  <span class="recent-name">{ch.name}</span>
                  <span class="recent-meta">{ch.group_name}</span>
                </div>
                <span class="recent-type" class:live={ch.content_type === 'live'} class:vod={ch.content_type === 'vod'} class:series={ch.content_type === 'series'}>
                  {ch.content_type}
                </span>
              </button>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Quick Favorites -->
      <section class="section">
        <div class="section-header">
          <h2 class="section-title">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
            Favorites
          </h2>
          {#if recentFavorites.length > 0}
            <button class="section-link" onclick={() => goto('/favorites')}>View all</button>
          {/if}
        </div>
        {#if recentFavorites.length === 0}
          <div class="section-empty">No favorites yet</div>
        {:else}
          <div class="fav-chips">
            {#each recentFavorites.slice(0, 12) as fav (fav.id)}
              <button class="fav-chip" onclick={() => goto('/favorites')}>
                {#if fav.logo_url}
                  <img class="fav-chip-icon" src={fav.logo_url} alt="" />
                {/if}
                <span>{fav.channel_name}</span>
              </button>
            {/each}
          </div>
        {/if}
      </section>
    </div>

    <!-- New Movies -->
    {#if newVod.length > 0}
      <section class="section">
        <div class="section-header">
          <h2 class="section-title">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            New Movies
          </h2>
          <button class="section-link" onclick={() => goto('/playlists')}>View all</button>
        </div>
        <div class="poster-scroll">
          {#each newVod as ch (ch.id)}
            <button class="poster-card" onclick={() => goto('/playlists')}>
              {#if ch.logo_url}
                <img class="poster-img" src={ch.logo_url} alt="" loading="lazy" />
              {:else}
                <div class="poster-img placeholder">
                  <span>{ch.name.charAt(0).toUpperCase()}</span>
                </div>
              {/if}
              <div class="poster-info">
                <span class="poster-title">{ch.name}</span>
                <span class="poster-group">{ch.group_name}</span>
              </div>
              <div class="poster-badge">NEW</div>
            </button>
          {/each}
        </div>
      </section>
    {/if}

    <!-- New Series -->
    {#if newSeries.length > 0}
      <section class="section">
        <div class="section-header">
          <h2 class="section-title">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="2" width="20" height="20" rx="2"/><path d="M7 2v20M17 2v20M2 12h20"/></svg>
            New Series
          </h2>
          <button class="section-link" onclick={() => goto('/playlists')}>View all</button>
        </div>
        <div class="poster-scroll">
          {#each newSeries as ch (ch.id)}
            <button class="poster-card" onclick={() => goto('/playlists')}>
              {#if ch.logo_url}
                <img class="poster-img" src={ch.logo_url} alt="" loading="lazy" />
              {:else}
                <div class="poster-img placeholder">
                  <span>{ch.name.charAt(0).toUpperCase()}</span>
                </div>
              {/if}
              <div class="poster-info">
                <span class="poster-title">{ch.name}</span>
                <span class="poster-group">{ch.group_name}</span>
              </div>
              <div class="poster-badge series">NEW</div>
            </button>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Playlists / Accounts -->
    <section class="section">
      <h2 class="section-title">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6h16M4 10h16M4 14h10"/><path d="M18 14v6M15 17h6"/></svg>
        Playlists
      </h2>
      <div class="accounts-grid">
        {#each $playlists as p (p.id)}
          {@const info = accountInfoMap[p.id]}
          {@const days = info ? daysUntilExpiry(info.exp_date) : null}
          <button class="account-card" onclick={() => goto('/playlists')}>
            <div class="account-header">
              <div class="account-type-badge" class:xtream={p.source_type === 'xtream'} class:m3u={p.source_type !== 'xtream'}>
                {p.source_type === 'xtream' ? 'Xtream' : 'M3U'}
              </div>
              <h3 class="account-name">{p.name}</h3>
              {#if info && days !== null}
                <span class="account-expiry" class:warn={days <= 7 && days > 0} class:expired={days <= 0}>
                  {days > 0 ? `${days}d` : 'Exp'}
                </span>
              {/if}
            </div>
            <div class="account-counts">
              <div class="count-item"><span class="count-value">{getTotalCount(p.id)}</span><span class="count-label">Total</span></div>
              <div class="count-item"><span class="count-value">{getCount(p.id, 'live')}</span><span class="count-label">Live</span></div>
              <div class="count-item"><span class="count-value">{getCount(p.id, 'vod')}</span><span class="count-label">VOD</span></div>
              <div class="count-item"><span class="count-value">{getCount(p.id, 'series')}</span><span class="count-label">Series</span></div>
            </div>
            {#if info}
              <div class="account-status">
                <span class="status-dot" class:active={info.status === 'Active'}></span>
                <span>{info.status}</span>
                <span class="status-sep">&middot;</span>
                <span>{info.active_cons ?? '0'}/{info.max_connections ?? '?'} conn</span>
                <span class="status-sep">&middot;</span>
                <span>{formatExpDate(info.exp_date)}</span>
              </div>
            {/if}
          </button>
        {/each}
      </div>
    </section>
  {/if}
</div>

{#if showAddModal}
  <AddPlaylistModal onclose={onPlaylistAdded} />
{/if}

<style>
  .dashboard { display: flex; flex-direction: column; gap: 24px; padding-bottom: 40px; }

  .page-header { display: flex; align-items: center; justify-content: space-between; }
  .page-title { font-size: 24px; font-weight: 700; }
  .page-subtitle { font-size: 13px; color: var(--color-text-muted); }
  .add-btn { display: flex; align-items: center; gap: 6px; padding: 8px 16px; font-size: 13px; font-weight: 600; }
  .add-btn :global(svg) { width: 16px; height: 16px; }

  .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 14px; }

  .empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 80px 0; color: var(--color-text-muted); }
  .empty-icon :global(svg) { width: 56px; height: 56px; opacity: 0.3; }
  .empty-state h2 { font-size: 18px; font-weight: 600; color: var(--color-text); }
  .empty-state p { font-size: 14px; margin-bottom: 8px; }

  .dashboard-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; }

  /* Sections */
  .section { display: flex; flex-direction: column; gap: 12px; }
  .section-header { display: flex; align-items: center; justify-content: space-between; }
  .section-title { display: flex; align-items: center; gap: 8px; font-size: 16px; font-weight: 600; }
  .section-title :global(svg) { width: 18px; height: 18px; color: var(--color-text-muted); }
  .section-link { font-size: 12px; color: var(--color-accent); background: transparent; font-weight: 500; }
  .section-link:hover { text-decoration: underline; }
  .section-empty { font-size: 13px; color: var(--color-text-muted); padding: 20px; text-align: center; background: var(--color-card); border-radius: var(--radius-card); border: 1px solid var(--color-border); }

  /* Recently watched */
  .recent-list { display: flex; flex-direction: column; gap: 2px; background: var(--color-card); border: 1px solid var(--color-border); border-radius: var(--radius-card); padding: 6px; }
  .recent-item { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 8px; background: transparent; color: var(--color-text); text-align: left; width: 100%; transition: background var(--transition-fast); }
  .recent-item:hover { background: var(--color-hover); }
  .recent-icon { width: 36px; height: 36px; border-radius: 8px; object-fit: contain; background: var(--color-surface); flex-shrink: 0; }
  .recent-icon.placeholder { display: flex; align-items: center; justify-content: center; }
  .recent-icon.placeholder span { font-size: 14px; font-weight: 700; color: var(--color-accent); opacity: 0.5; }
  .recent-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .recent-name { font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .recent-meta { font-size: 11px; color: var(--color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .recent-type { font-size: 10px; font-weight: 600; padding: 2px 6px; border-radius: 4px; text-transform: uppercase; flex-shrink: 0; }
  .recent-type.live { background: rgba(233, 69, 96, 0.12); color: var(--color-accent); }
  .recent-type.vod { background: rgba(123, 104, 238, 0.12); color: var(--color-accent-purple); }
  .recent-type.series { background: rgba(78, 204, 163, 0.12); color: var(--color-accent-green); }

  /* Favorite chips */
  .fav-chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .fav-chip { display: flex; align-items: center; gap: 6px; padding: 6px 12px; border-radius: 20px; background: var(--color-card); border: 1px solid var(--color-border); color: var(--color-text); font-size: 12px; font-weight: 500; transition: all var(--transition-fast); }
  .fav-chip:hover { border-color: var(--color-accent); color: var(--color-accent); }
  .fav-chip-icon { width: 20px; height: 20px; border-radius: 4px; object-fit: contain; background: var(--color-surface); }

  /* Poster scroll — horizontal scrollable movie/series cards */
  .poster-scroll {
    display: flex;
    gap: 14px;
    overflow-x: auto;
    padding: 4px 0 8px;
    scroll-snap-type: x mandatory;
    -webkit-overflow-scrolling: touch;
  }

  .poster-scroll::-webkit-scrollbar {
    height: 4px;
  }

  .poster-scroll::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: 2px;
  }

  .poster-card {
    flex: 0 0 150px;
    display: flex;
    flex-direction: column;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
    text-align: left;
    scroll-snap-align: start;
    position: relative;
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .poster-card:hover {
    transform: translateY(-4px) scale(1.02);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.4);
    border-color: var(--color-accent);
  }

  .poster-img {
    width: 100%;
    aspect-ratio: 2 / 3;
    object-fit: cover;
    background: var(--color-surface);
  }

  .poster-img.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--color-card), var(--color-surface));
  }

  .poster-img.placeholder span {
    font-size: 36px;
    font-weight: 700;
    color: var(--color-accent);
    opacity: 0.25;
  }

  .poster-info {
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .poster-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .poster-group {
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .poster-badge {
    position: absolute;
    top: 8px;
    right: 8px;
    font-size: 9px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 4px;
    background: var(--color-accent);
    color: #fff;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }

  .poster-badge.series {
    background: var(--color-accent-green);
  }

  /* Accounts */
  .accounts-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 14px; }
  .account-card { background: var(--color-card); border: 1px solid var(--color-border); border-radius: var(--radius-card); padding: 16px; display: flex; flex-direction: column; gap: 12px; text-align: left; transition: all var(--transition-fast); width: 100%; }
  .account-card:hover { transform: translateY(-2px); box-shadow: 0 8px 25px rgba(0,0,0,0.3); }
  .account-header { display: flex; align-items: center; gap: 8px; }
  .account-type-badge { padding: 2px 7px; border-radius: 5px; font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; flex-shrink: 0; }
  .account-type-badge.xtream { background: rgba(123,104,238,0.15); color: var(--color-accent-purple); }
  .account-type-badge.m3u { background: rgba(78,204,163,0.15); color: var(--color-accent-green); }
  .account-name { font-size: 14px; font-weight: 600; flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .account-expiry { font-size: 10px; font-weight: 700; padding: 2px 6px; border-radius: 4px; background: rgba(78,204,163,0.12); color: var(--color-accent-green); flex-shrink: 0; }
  .account-expiry.warn { background: rgba(240,165,0,0.12); color: var(--color-accent-yellow); }
  .account-expiry.expired { background: rgba(233,69,96,0.12); color: var(--color-accent); }

  .account-counts { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; background: var(--color-surface); border-radius: 8px; padding: 10px 6px; }
  .count-item { display: flex; flex-direction: column; align-items: center; gap: 1px; }
  .count-value { font-size: 16px; font-weight: 700; }
  .count-label { font-size: 10px; color: var(--color-text-muted); }

  .account-status { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--color-text-muted); }
  .status-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--color-text-muted); flex-shrink: 0; }
  .status-dot.active { background: var(--color-accent-green); }
  .status-sep { opacity: 0.4; }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }
</style>
