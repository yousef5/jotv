<script lang="ts" module>
  // Where you were, so leaving Live TV and coming back lands in the same spot
  const listScroll = new Map<string, number>();
  let lastView: { pid: number; view: string } | null = null;
  // Channels per category, for the "More in …" strip
  const groupCache = new Map<string, import('$lib/tauri').Channel[]>();
</script>

<script lang="ts">
  import { askFavoriteCategory, favoriteFiled } from '$lib/components/FavoriteSaved.svelte';
  import { CATEGORY_COLORS } from '$lib/stores/reels';
  import { onMount, tick, untrack } from 'svelte';
  import { get } from 'svelte/store';
  import { jsFade } from '$lib/motion';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import {
    getGroupsByType, getChannelsByGroup, searchChannelsInPlaylist, getFavorites, getRecentlyWatched, favoriteLists,
    toggleFavorite, probeStream, mpvPause, mpvFullscreen, getSetting, setSetting,
  } from '$lib/tauri';
  import type { Channel, ChannelGroup, FavoriteList } from '$lib/tauri';
  import { playlists, loadPlaylists, pendingOpen } from '$lib/stores/playlists';
  import TrackPanel from '$lib/components/TrackPanel.svelte';
  import { accounts, loadAccount, connectionsFull } from '$lib/stores/account';
  import { streamGuard, reloadStream, setStable, flash } from '$lib/stores/streamGuard';
  import { SYNC_STEP_MS, syncLabel } from '$lib/audioSync';
  import {
    nowPlaying, liveStatus, liveError, liveMode, liveFullscreen, videoAnchor, liveBehind, liveVolume, liveMuted,
    BEHIND_LIMIT, playLive, stopLive, setLiveMode, goLive, setVolume, changeVolume, toggleMute, shiftAudio,
  } from '$lib/stores/live';

  const PAGE = 150;
  const FAVORITES = '__favorites__';
  const RECENT = '__recent__';
  /** One of your favorite categories: `__fav:12` */
  const FAV_LIST = '__fav:';

  let pid = $state(0);
  let view = $state<string>(FAVORITES);
  let groups = $state<ChannelGroup[]>([]);
  let channels = $state<Channel[]>([]);
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let favIds = $state<Set<number>>(new Set());
  let favChannels = $state<Channel[]>([]);
  /** Your channel categories, and which one each favorite is in */
  let favLists = $state<FavoriteList[]>([]);
  let favListOf = $state<Map<number, number | null>>(new Map());

  function listIdOf(ch: Channel): number | null {
    const id = favListOf.get(ch.id) ?? null;
    return id !== null && favLists.some((l) => l.id === id) ? id : null;
  }
  function listColor(l: FavoriteList): string {
    return l.color ?? CATEGORY_COLORS[l.id % CATEGORY_COLORS.length];
  }
  function viewList(v: string): FavoriteList | undefined {
    return v.startsWith(FAV_LIST) ? favLists.find((l) => l.id === Number(v.slice(FAV_LIST.length))) : undefined;
  }
  /** Favorites in category order (yours first, unsorted last) */
  function orderedFavs(): Channel[] {
    const rank = (ch: Channel) => {
      const id = listIdOf(ch);
      const i = favLists.findIndex((l) => l.id === id);
      return i < 0 ? favLists.length : i;
    };
    return [...favChannels].sort((a, b) => rank(a) - rank(b));
  }
  function favCount(listId: number): number {
    return favChannels.filter((c) => listIdOf(c) === listId).length;
  }
  let recentChannels = $state<Channel[]>([]);
  let query = $state('');
  let results = $state<Channel[] | null>(null);
  let highlighted = $state<number | null>(null);
  let probe = $state<Record<number, 'testing' | 'ok' | 'dead'>>({});
  let groupFilter = $state('');
  // Not remembered: coming back to Live TV always shows the channel list
  let theater = $state(false);
  let showKeys = $state(false);
  let showTracks = $state(false);
  let clockNow = $state(Date.now());
  $effect(() => {
    if ($streamGuard.phase === 'ok') return;
    const t = setInterval(() => (clockNow = Date.now()), 1000);
    return () => clearInterval(t);
  });
  let retryIn = $derived(
    $streamGuard.nextRetryAt ? Math.max(0, Math.ceil(($streamGuard.nextRetryAt - clockNow) / 1000)) : 0,
  );
  const HEALTH_LABEL = { good: 'Strong', fair: 'Unsteady', poor: 'Weak' } as const;
  let strip = $state<Channel[]>([]);

  let listEl = $state<HTMLDivElement | undefined>();
  let screenEl = $state<HTMLDivElement | undefined>();
  let searchEl = $state<HTMLInputElement | undefined>();
  let drawerEl = $state<HTMLDivElement | undefined>();
  let filterEl = $state<HTMLInputElement | undefined>();
  let catListEl = $state<HTMLElement | undefined>();
  let catsOpen = $state(false);
  let token = 0;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  let playlist = $derived($playlists.find((p) => p.id === pid) ?? null);
  // A movie playing elsewhere doesn't belong on this page
  let liveNow = $derived($nowPlaying?.kind === 'live' ? $nowPlaying : null);
  let shown = $derived(results ?? channels);
  // The stage shows what's playing (its screen holds the video); otherwise the highlighted
  // channel, or the first one in the list so the stage is never an empty page
  let current = $derived(liveNow?.channel ?? shown.find((c) => c.id === highlighted) ?? shown[0] ?? null);
  // The row the keyboard is on: what you moved to, else what's on stage
  let focusId = $derived(highlighted !== null && shown.some((c) => c.id === highlighted) ? highlighted : (current?.id ?? null));
  // Showing one of the provider's categories (not favorites, recent or a search)
  let inGroup = $derived(view !== FAVORITES && view !== RECENT && !view.startsWith(FAV_LIST));
  let embedded = $derived(!!liveNow?.embedded);
  let isPlayingCurrent = $derived(!!current && liveNow?.channel.id === current.id);
  // All favorites shown under your category headings
  let groupedFavs = $derived(view === FAVORITES && !results && favLists.some((l) => favCount(l.id) > 0));
  let viewTitle = $derived(
    results
      ? `Results for “${query.trim()}”`
      : view === FAVORITES ? 'Favorites'
      : view === RECENT ? 'Recently watched'
      : view.startsWith(FAV_LIST) ? (viewList(view)?.name ?? 'Favorites')
      : view || 'Uncategorized',
  );
  let filteredGroups = $derived(
    groupFilter.trim() ? groups.filter((g) => g.name.toLowerCase().includes(groupFilter.trim().toLowerCase())) : groups,
  );
  let totalLive = $derived(groups.reduce((n, g) => n + g.count, 0));
  let playingId = $derived(liveNow?.channel.id ?? null);
  let playingHidden = $derived(!!playingId && !shown.some((c) => c.id === playingId));
  let behind = $derived(isPlayingCurrent && $liveStatus === 'live' && $liveBehind > BEHIND_LIMIT);
  let chNumber = $derived(current ? strip.findIndex((c) => c.id === current!.id) + 1 : 0);

  // The drawer slides by rAF, not a CSS transition: when an accelerated transition
  // ends, WebKitGTK paints one frame at the start position and the drawer blinks.
  const DRAWER_MS = 200;
  let drawerPos = 0; // 0 = hidden, 1 = open
  let drawerRaf = 0;
  $effect(() => {
    const target = catsOpen ? 1 : 0;
    const el = drawerEl;
    if (!el) return;
    cancelAnimationFrame(drawerRaf);
    const from = drawerPos;
    if (from === target) return;
    const t0 = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - t0) / (DRAWER_MS * Math.abs(target - from)));
      const eased = 1 - Math.pow(1 - k, 3);
      drawerPos = from + (target - from) * eased;
      el.style.transform = `translate3d(${(drawerPos - 1) * 101}%, 0, 0)`;
      if (k < 1) drawerRaf = requestAnimationFrame(step);
    };
    drawerRaf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(drawerRaf);
  });

  async function openCats() {
    catsOpen = true;
    groupFilter = '';
    await tick();
    // Show the open category: scroll the list itself, never scrollIntoView
    const on = catListEl?.querySelector<HTMLElement>('.cat.on');
    if (catListEl) catListEl.scrollTop = on ? on.offsetTop - catListEl.clientHeight / 2 : 0;
    // preventScroll: a plain focus() makes WebKit scroll the half-slid drawer into view
    filterEl?.focus({ preventScroll: true });
  }

  function pickGroup(name: string) {
    catsOpen = false;
    openView(name);
  }

  function toggleTheater() {
    theater = !theater;
  }

  // A channel that won't play may mean every connection is in use: re-check the account
  $effect(() => {
    if (isPlayingCurrent && ($liveStatus === 'nosignal' || $liveStatus === 'offline') && playlist) {
      loadAccount(playlist, true);
    }
  });

  // "More in <category>" strip follows the channel on stage
  $effect(() => {
    const group = current?.group_name;
    const p = pid;
    if (!p || group === undefined) { strip = []; return; }
    const key = `${p}|${group}`;
    const cached = groupCache.get(key);
    if (cached) { strip = cached; return; }
    getChannelsByGroup(p, 'live', group, 80, 0)
      .then((list) => {
        groupCache.set(key, list);
        if (current?.group_name === group) strip = list;
      })
      .catch(() => {});
  });

  async function jumpToPlaying() {
    const ch = liveNow?.channel;
    if (!ch) return;
    await openView(ch.group_name);
    highlighted = ch.id;
    scrollToHighlighted();
  }

  const nf = new Intl.NumberFormat('en-US');

  const STATUS: Record<string, { label: string; tone: string }> = {
    starting: { label: 'Starting…', tone: 'wait' },
    live: { label: 'Live', tone: 'live' },
    buffering: { label: 'Buffering…', tone: 'wait' },
    nosignal: { label: 'No signal', tone: 'bad' },
    paused: { label: 'Paused', tone: 'idle' },
    offline: { label: 'Offline', tone: 'bad' },
  };

  // In-app video sits over the stage screen while this page is open
  $effect(() => {
    videoAnchor.set(screenEl ? { el: screenEl, kind: 'live' } : null);
    return () => videoAnchor.set(null);
  });

  function fullscreen() {
    if (liveNow?.embedded) liveFullscreen.set(true);
    else mpvFullscreen().catch(() => {});
  }

  onMount(() => {
    init();
    return () => {
      if (listEl) listScroll.set(`${pid}|${view}`, listEl.scrollTop);
      lastView = { pid, view };
      clearTimeout(searchTimer);
    };
  });

  async function init() {
    if (!$playlists.length) await loadPlaylists();
    const pending = get(pendingOpen);
    pendingOpen.set(null);

    const urlPid = parseInt($page.url.searchParams.get('pid') ?? '');
    let startPid = pending?.playlistId ?? (isNaN(urlPid) ? lastView?.pid : urlPid);
    if (!startPid) {
      const saved = parseInt((await getSetting('last_playlist_id').catch(() => null)) ?? '');
      startPid = isNaN(saved) ? $playlists[0]?.id : saved;
    }
    if (!startPid || !$playlists.some((p) => p.id === startPid)) startPid = $playlists[0]?.id;
    if (!startPid) { loading = false; return; }

    await switchPlaylist(startPid, pending?.channel?.group_name ?? (lastView?.pid === startPid ? lastView.view : undefined));

    const startQuery = $page.url.searchParams.get('q');
    if (startQuery) {
      query = startQuery;
      onSearchInput();
    }

    if (pending?.channel) {
      highlighted = pending.channel.id;
      await playLive(pending.channel);
      await tick();
      scrollToHighlighted();
    } else if (liveNow) {
      highlighted = liveNow.channel.id;
    }
  }

  async function switchPlaylist(id: number, startView?: string) {
    pid = id;
    setSetting('last_playlist_id', String(id)).catch(() => {});
    const [g, favs, recent, lists] = await Promise.all([
      getGroupsByType(id, 'live').catch(() => [] as ChannelGroup[]),
      getFavorites().catch(() => []),
      getRecentlyWatched(60).catch(() => [] as Channel[]),
      favoriteLists().catch(() => [] as FavoriteList[]),
    ]);
    favLists = lists.filter((l) => l.kind === 'live');
    favListOf = new Map(favs.map((f) => [f.channel_id, f.list_id]));
    groups = g.filter((x) => x.count > 0);
    const liveFavs = favs.filter((f) => f.content_type === 'live' && f.playlist_id === id);
    favIds = new Set(favs.map((f) => f.channel_id));
    favChannels = liveFavs.map((f) => ({
      id: f.channel_id, playlist_id: f.playlist_id, name: f.channel_name, group_name: f.group_name,
      stream_url: f.stream_url, logo_url: f.logo_url, epg_id: null, content_type: 'live', created_at: f.added_at,
    }));
    recentChannels = recent.filter((c) => c.content_type === 'live' && c.playlist_id === id);

    const fallback = favChannels.length ? FAVORITES : recentChannels.length ? RECENT : groups[0]?.name ?? FAVORITES;
    const known = startView === FAVORITES || startView === RECENT || !!(startView && viewList(startView)) || groups.some((x) => x.name === startView);
    await openView(startView && known ? startView : fallback);
  }

  async function openView(v: string) {
    const t = ++token;
    view = v;
    clearSearch(false);
    hasMore = false;
    if (v === FAVORITES) {
      channels = orderedFavs();
      loading = false;
    } else if (v.startsWith(FAV_LIST)) {
      const id = Number(v.slice(FAV_LIST.length));
      channels = favChannels.filter((c) => listIdOf(c) === id);
      loading = false;
    } else if (v === RECENT) {
      channels = recentChannels;
      loading = false;
    } else {
      loading = true;
      channels = [];
      const batch = await getChannelsByGroup(pid, 'live', v, PAGE, 0).catch(() => [] as Channel[]);
      if (t !== token) return;
      channels = batch;
      hasMore = batch.length === PAGE;
      loading = false;
    }
    await tick();
    if (listEl) listEl.scrollTop = listScroll.get(`${pid}|${v}`) ?? 0;
  }

  async function loadMore() {
    if (!hasMore || loadingMore || results || view === FAVORITES || view === RECENT || view.startsWith(FAV_LIST)) return;
    const t = token;
    loadingMore = true;
    const batch = await getChannelsByGroup(pid, 'live', view, PAGE, channels.length).catch(() => [] as Channel[]);
    if (t === token) {
      channels = [...channels, ...batch];
      hasMore = batch.length === PAGE;
    }
    loadingMore = false;
  }

  function onListScroll() {
    if (!listEl) return;
    if (listEl.scrollTop + listEl.clientHeight > listEl.scrollHeight - 600) loadMore();
  }

  // ── Search ──────────────────────────────────────────────────────────────────

  function onSearchInput() {
    clearTimeout(searchTimer);
    const term = query.trim();
    if (!term) { results = null; return; }
    searchTimer = setTimeout(async () => {
      const r = await searchChannelsInPlaylist(pid, 'live', term, 200).catch(() => [] as Channel[]);
      if (query.trim() === term) {
        results = r;
        highlighted = r[0]?.id ?? highlighted;
        if (listEl) listEl.scrollTop = 0;
      }
    }, 180);
  }

  function clearSearch(focus = true) {
    query = '';
    results = null;
    clearTimeout(searchTimer);
    if (focus) searchEl?.focus({ preventScroll: true });
  }

  // ── Playback ────────────────────────────────────────────────────────────────

  async function play(ch: Channel) {
    highlighted = ch.id;
    // Keep "Recently watched" current without a reload
    recentChannels = [ch, ...recentChannels.filter((c) => c.id !== ch.id)].slice(0, 60);
    if (view === RECENT && !results) channels = recentChannels;
    await playLive(ch);
  }

  function zap(dir: 1 | -1) {
    const list = shown;
    if (!list.length) return;
    const from = list.findIndex((c) => c.id === (liveNow?.channel.id ?? highlighted));
    const next = list[(from + dir + list.length) % list.length];
    play(next).then(scrollToHighlighted);
  }

  function move(dir: 1 | -1) {
    const list = shown;
    if (!list.length) return;
    const i = list.findIndex((c) => c.id === focusId);
    const next = list[Math.min(list.length - 1, Math.max(0, i === -1 ? 0 : i + dir))];
    highlighted = next.id;
    scrollToHighlighted();
    if (list.indexOf(next) > list.length - 10) loadMore();
  }

  async function scrollToHighlighted() {
    await tick();
    // Scroll the list itself: scrollIntoView would also nudge the page around it
    const row = listEl?.querySelector<HTMLElement>(`[data-id="${highlighted}"]`);
    if (!listEl || !row) return;
    const top = row.offsetTop;
    if (top < listEl.scrollTop + 8) listEl.scrollTop = top - 8;
    else if (top + row.offsetHeight > listEl.scrollTop + listEl.clientHeight - 8) listEl.scrollTop = top + row.offsetHeight - listEl.clientHeight + 8;
  }

  async function toggleFav(ch: Channel, anchor?: EventTarget | null) {
    const on = await toggleFavorite(ch.id).catch(() => favIds.has(ch.id));
    const next = new Set(favIds);
    if (on) {
      next.add(ch.id);
      askFavoriteCategory(ch, anchor as Element | null);
      if (!favChannels.some((c) => c.id === ch.id)) favChannels = [...favChannels, ch];
    } else {
      next.delete(ch.id);
      favChannels = favChannels.filter((c) => c.id !== ch.id);
      if ((view === FAVORITES || view.startsWith(FAV_LIST)) && !results) refreshFavView();
    }
    favIds = next;
  }

  function refreshFavView() {
    if (view === FAVORITES) channels = orderedFavs();
    else if (view.startsWith(FAV_LIST)) {
      const id = Number(view.slice(FAV_LIST.length));
      channels = favChannels.filter((c) => listIdOf(c) === id);
    }
  }

  // Filed into a category from the "Saved" popover: show it in place
  $effect(() => {
    const f = $favoriteFiled;
    if (!f) return;
    untrack(async () => {
      if (f.listId !== null && !favLists.some((l) => l.id === f.listId)) {
        favLists = (await favoriteLists().catch(() => [] as FavoriteList[])).filter((l) => l.kind === 'live');
      }
      favListOf = new Map(favListOf).set(f.channelId, f.listId);
      if (!results) refreshFavView();
    });
  });

  async function test(ch: Channel) {
    probe = { ...probe, [ch.id]: 'testing' };
    const r = await probeStream(ch.stream_url).catch(() => ({ ok: false }));
    probe = { ...probe, [ch.id]: r.ok ? 'ok' : 'dead' };
  }

  function onKey(e: KeyboardEvent) {
    const inSearch = e.target === searchEl;
    if (e.key === 'Escape' && catsOpen) {
      e.preventDefault();
      catsOpen = false;
      return;
    }
    const typing = (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) && !inSearch;
    if (typing) return;
    if (!inSearch && e.key === 'g') {
      e.preventDefault();
      catsOpen ? (catsOpen = false) : openCats();
      return;
    }

    if ((e.key === 'k' && (e.ctrlKey || e.metaKey)) || (e.key === '/' && !inSearch)) {
      e.preventDefault();
      searchEl?.focus({ preventScroll: true });
      searchEl?.select();
    } else if (e.key === 'ArrowDown' && !e.ctrlKey) {
      e.preventDefault();
      move(1);
    } else if (e.key === 'ArrowUp' && !e.ctrlKey) {
      e.preventDefault();
      move(-1);
    } else if (e.key === 'PageDown' || (e.ctrlKey && e.key === 'ArrowDown')) {
      e.preventDefault();
      zap(1);
    } else if (e.key === 'PageUp' || (e.ctrlKey && e.key === 'ArrowUp')) {
      e.preventDefault();
      zap(-1);
    } else if (e.key === 'Enter') {
      const ch = shown.find((c) => c.id === focusId);
      if (ch && ch.id !== liveNow?.channel.id) { e.preventDefault(); play(ch); }
    } else if (e.key === 'Escape' && inSearch && query) {
      e.preventDefault();
      clearSearch();
    } else if (e.key === 'Escape' && showKeys) {
      showKeys = false;
    } else if (e.key === 'Escape' && theater) {
      theater = false;
    } else if (!inSearch && e.key === 't') {
      toggleTheater();
    } else if (!inSearch && e.key === '?') {
      showKeys = !showKeys;
    } else if (!inSearch && e.key === 'c' && liveNow) {
      showTracks = !showTracks;
    } else if (!inSearch && (e.key === '[' || e.key === ']') && liveNow) {
      // Sound ahead of the picture on this channel? ] plays it later
      flash(syncLabel(shiftAudio(e.key === ']' ? SYNC_STEP_MS : -SYNC_STEP_MS)));
    } else if (!inSearch && liveNow) {
      if (e.key === ' ') { e.preventDefault(); mpvPause().catch(() => {}); }
      else if (e.key === 'f') fullscreen();
      else if (e.key === 'l') goLive();
      else if (e.key === 'r') reloadStream();
      else if (e.key === 'm') toggleMute();
      else if (e.key === '+' || e.key === '=') changeVolume(5);
      else if (e.key === '-') changeVolume(-5);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- Theater only applies while a channel plays; otherwise it would hide the lists and leave an empty page -->
<div class="live-page" class:theater={theater && !!liveNow}>
  <!-- Guide: one panel for finding a channel; categories slide over it -->
  <section class="guide">
    <header class="guide-head">
      <div class="title-row">
        <h1><i class="live-dot" aria-hidden="true"></i>Live TV</h1>
        {#if $playlists.length > 1}
          <select class="pl-select" value={pid} onchange={(e) => switchPlaylist(+(e.currentTarget as HTMLSelectElement).value)} aria-label="Playlist">
            {#each $playlists as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
          </select>
        {:else if playlist}
          <span class="pl-name">{nf.format(totalLive)} channels</span>
        {/if}
      </div>

      <div class="search">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
        <input
          bind:this={searchEl}
          bind:value={query}
          oninput={onSearchInput}
          type="search"
          placeholder="Search all channels"
          aria-label="Search all channels"
          autocomplete="off"
          spellcheck="false"
          dir="auto"
        />
        {#if query}
          <button class="clear" onclick={() => clearSearch()} aria-label="Clear search">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        {:else}
          <kbd>Ctrl K</kbd>
        {/if}
      </div>

      <button class="cats-btn" class:on={inGroup && !results} onclick={openCats} aria-haspopup="dialog" aria-expanded={catsOpen}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" aria-hidden="true"><rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/></svg>
        <span class="cats-btn-text">
          <small>Category</small>
          <b dir="auto">{inGroup ? view || 'Uncategorized' : `All ${nf.format(groups.length)} categories`}</b>
        </span>
        <kbd>G</kbd>
      </button>

      <div class="tabs" aria-label="Your channels">
        <button class="tab" class:on={view === FAVORITES && !results} onclick={() => openView(FAVORITES)}>
          <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 21s-7.5-4.6-9.6-9.4C.9 8.1 3.2 4.5 6.9 4.5c2.1 0 3.6 1.1 5.1 3 1.5-1.9 3-3 5.1-3 3.7 0 6 3.6 4.5 7.1C19.5 16.4 12 21 12 21z"/></svg>
          Favorites <span>{favChannels.length}</span>
        </button>
        <button class="tab" class:on={view === RECENT && !results} onclick={() => openView(RECENT)}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 15.5 14"/></svg>
          Recent <span>{recentChannels.length}</span>
        </button>
        {#each favLists as l (l.id)}
          {@const v = `${FAV_LIST}${l.id}`}
          <button class="tab" class:on={view === v && !results} onclick={() => openView(v)} title={l.name}>
            <i class="cat-dot" style:background={listColor(l)}></i>
            <span class="tab-name" dir="auto">{l.name}</span> <span>{favCount(l.id)}</span>
          </button>
        {/each}
      </div>
    </header>

    <header class="list-head">
      <h2 dir="auto">{viewTitle}</h2>
      <span>{nf.format(shown.length)}{hasMore && !results ? '+' : ''}</span>
    </header>

    {#if playingHidden && liveNow}
      <button class="np-row" onclick={jumpToPlaying} title="Show it in its category">
        <span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>
        <span class="np-text">
          <span class="np-label">Now playing</span>
          <span class="np-name" dir="auto">{liveNow.channel.name}</span>
        </span>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="9 6 15 12 9 18"/></svg>
      </button>
    {/if}

    <div class="list" bind:this={listEl} onscroll={onListScroll} role="listbox" aria-label="Channels" tabindex="-1">
      {#if loading}
        {#each Array(10) as _, i (i)}
          <div class="row sk-row" aria-hidden="true"><span class="sk sk-logo"></span><span class="sk sk-text"></span></div>
        {/each}
      {:else if !shown.length}
        <div class="empty">
          {#if results}
            <strong>No channels match “{query.trim()}”</strong>
            <span>Try part of the name, like "bein" or "mbc".</span>
          {:else if view === FAVORITES}
            <strong>No favorite channels yet</strong>
            <span>Hover a channel and press ♡ to pin it here.</span>
          {:else if view.startsWith(FAV_LIST)}
            <strong>No channels in this category yet</strong>
            <span>Press ♡ on a channel and pick “{viewList(view)?.name}”, or sort them on the My list page.</span>
          {:else if view === RECENT}
            <strong>Nothing watched yet</strong>
            <span>Channels you play show up here.</span>
          {:else}
            <strong>This category is empty</strong>
          {/if}
        </div>
      {:else}
        {#each shown as ch, i (ch.id)}
          {@const playing = liveNow?.channel.id === ch.id}
          {#if groupedFavs && (i === 0 || listIdOf(shown[i - 1]) !== listIdOf(ch))}
            {@const l = favLists.find((x) => x.id === listIdOf(ch))}
            <div class="fav-head">
              {#if l}
                <i class="cat-dot" style:background={listColor(l)}></i>
                <button onclick={() => openView(`${FAV_LIST}${l.id}`)} dir="auto">{l.name}</button>
              {:else}
                <span>Not sorted</span>
              {/if}
            </div>
          {/if}
          <div
            class="row"
            class:hl={focusId === ch.id}
            class:playing
            data-id={ch.id}
            role="option"
            aria-selected={focusId === ch.id}
          >
            <button class="row-main" onclick={() => play(ch)} onfocus={() => (highlighted = ch.id)}>
              <span class="num">{i + 1}</span>
              <span class="logo">
                {#if ch.logo_url}
                  <img src={ch.logo_url} alt="" loading="lazy" />
                {:else}
                  <span class="logo-letter">{ch.name.trim().charAt(0).toUpperCase()}</span>
                {/if}
              </span>
              <span class="row-text">
                <span class="row-name" dir="auto">{ch.name}</span>
                {#if playing}
                  <span class="row-sub live-sub"><i></i>Watching now</span>
                {:else if results || view === FAVORITES || view === RECENT || view.startsWith(FAV_LIST)}
                  <span class="row-sub" dir="auto">{ch.group_name}</span>
                {/if}
              </span>
              {#if playing}
                <span class="eq" aria-label="Now playing"><i></i><i></i><i></i></span>
              {/if}
            </button>
            <button
              class="row-fav"
              class:on={favIds.has(ch.id)}
              onclick={(e) => toggleFav(ch, e.currentTarget)}
              aria-label={favIds.has(ch.id) ? `Remove ${ch.name} from favorites` : `Add ${ch.name} to favorites`}
            >
              <svg viewBox="0 0 24 24" fill={favIds.has(ch.id) ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2"><path d="M12 21s-7.5-4.6-9.6-9.4C.9 8.1 3.2 4.5 6.9 4.5c2.1 0 3.6 1.1 5.1 3 1.5-1.9 3-3 5.1-3 3.7 0 6 3.6 4.5 7.1C19.5 16.4 12 21 12 21z"/></svg>
            </button>
          </div>
        {/each}
        {#if loadingMore}<div class="more"><span class="spinner"></span></div>{/if}
      {/if}
    </div>

    <!-- Categories drawer: always mounted so it opens instantly; slid by script (see DRAWER_MS) -->
    <div class="drawer" bind:this={drawerEl} role="dialog" aria-label="Categories" inert={!catsOpen}>
      <header class="drawer-head">
        <h2>Categories <span>{nf.format(groups.length)}</span></h2>
        <button class="drawer-x" onclick={() => (catsOpen = false)} aria-label="Close categories">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </header>
      <div class="search drawer-search">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
        <input
          bind:this={filterEl}
          bind:value={groupFilter}
          onkeydown={(e) => { if (e.key === 'Enter' && filteredGroups[0]) pickGroup(filteredGroups[0].name); }}
          placeholder="Filter categories"
          aria-label="Filter categories"
          autocomplete="off"
          spellcheck="false"
          dir="auto"
        />
      </div>
      <nav class="cat-list" bind:this={catListEl}>
        {#each filteredGroups as g (g.name)}
          <button class="cat" class:on={view === g.name && !results} onclick={() => pickGroup(g.name)} title={g.name}>
            <span class="cat-name" dir="auto">{g.name || 'Uncategorized'}</span>
            <span class="cat-count">{nf.format(g.count)}</span>
          </button>
        {:else}
          <p class="cat-none">No category matches “{groupFilter.trim()}”</p>
        {/each}
      </nav>
    </div>
  </section>

  <!-- Stage -->
  <section class="stage">
    {#if current}
      <div class="stage-body">
        <div class="player dark-zone">
          <!-- Ambilight: the channel's colors glow around the screen -->
          {#key current.id}
            <div class="glow" aria-hidden="true" in:jsFade={{ duration: 500 }}>
              {#if current.logo_url}<img src={current.logo_url} alt="" />{/if}
            </div>
          {/key}
          <!-- In-app playback: MPV's native surface is placed exactly over this box -->
          <div class="screen" class:video={isPlayingCurrent && embedded} bind:this={screenEl}>
            {#if current.logo_url}
              <img class="screen-logo" src={current.logo_url} alt="" />
            {:else}
              <span class="screen-letter">{current.name.trim().charAt(0).toUpperCase()}</span>
            {/if}
            {#if !isPlayingCurrent}
              <button class="screen-play" onclick={() => play(current!)} aria-label={`Watch ${current.name}`}>
                <span class="watch-cta">
                  <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.14v13.72a1 1 0 001.5.86l11-6.86a1 1 0 000-1.72l-11-6.86A1 1 0 008 5.14z"/></svg>
                  Watch now
                  <kbd>Enter</kbd>
                </span>
              </button>
            {/if}
          </div>

          <div class="bar" role="toolbar" aria-label="Player controls">
            {#if theater && liveNow}
              <button class="channels-btn" onclick={toggleTheater} title="Show the channel list · Esc">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="14" y2="18"/></svg>
                Channels
              </button>
            {/if}
            {#if isPlayingCurrent}
              <button class="ib" onclick={() => mpvPause()} aria-label="Pause or resume" data-tip="Pause · Space">
                <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6.5" y="5" width="4" height="14" rx="1"/><rect x="13.5" y="5" width="4" height="14" rx="1"/></svg>
              </button>
            {:else}
              <button class="ib" onclick={() => play(current!)} aria-label="Watch" data-tip="Watch · Enter">
                <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.14v13.72a1 1 0 001.5.86l11-6.86a1 1 0 000-1.72l-11-6.86A1 1 0 008 5.14z"/></svg>
              </button>
            {/if}
            <button class="ib" onclick={() => zap(-1)} aria-label="Previous channel" data-tip="Previous channel · PgUp">
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M18 6.3v11.4a.8.8 0 01-1.2.7L9 13v4.5H7V6.5h2V11l7.8-5.4a.8.8 0 011.2.7z"/></svg>
            </button>
            <button class="ib" onclick={() => zap(1)} aria-label="Next channel" data-tip="Next channel · PgDn">
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 6.3v11.4a.8.8 0 001.2.7L15 13v4.5h2V6.5h-2V11L7.2 5.6A.8.8 0 006 6.3z"/></svg>
            </button>

            <div class="vol">
              <button class="ib" onclick={toggleMute} aria-label={$liveMuted ? 'Unmute' : 'Mute'} data-tip={$liveMuted ? 'Unmute · M' : 'Mute · M'}>
                {#if $liveMuted || $liveVolume === 0}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H3v6h3l5 4z" fill="currentColor"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/></svg>
                {:else}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H3v6h3l5 4z" fill="currentColor"/><path d="M15.5 8.5a5 5 0 010 7"/>{#if $liveVolume > 50}<path d="M18.5 5.5a9 9 0 010 13"/>{/if}</svg>
                {/if}
              </button>
              <input
                class="vol-slider"
                type="range"
                min="0"
                max="130"
                step="1"
                value={$liveMuted ? 0 : $liveVolume}
                style:--fill="{(($liveMuted ? 0 : $liveVolume) / 130) * 100}%"
                oninput={(e) => setVolume(+(e.currentTarget as HTMLInputElement).value)}
                aria-label="Volume"
              />
              <span class="vol-value">{$liveMuted ? 'Muted' : `${$liveVolume}%`}</span>
            </div>

            {#if isPlayingCurrent && $streamGuard.phase === 'recovering'}
              <span class="pill wait"><span class="mini-spin"></span>Reconnecting {$streamGuard.attempt}/{$streamGuard.maxAttempts}{#if retryIn > 0} · {retryIn}s{/if}</span>
            {:else if isPlayingCurrent && $streamGuard.phase === 'blocked'}
              <span class="pill bad"><i></i>Server paused · {Math.floor(retryIn / 60)}:{String(retryIn % 60).padStart(2, '0')}</span>
            {:else if isPlayingCurrent && $streamGuard.phase === 'failed'}
              <span class="pill bad"><i></i>Stream lost</span>
            {:else if isPlayingCurrent}
              {#if behind}
                <button class="pill behind" onclick={goLive} title="Jump to live · L"><i></i>Go live <small>−{$liveBehind}s</small></button>
              {:else if $liveStatus === 'live'}
                <span class="pill live"><i></i>LIVE</span>
              {:else if $liveStatus && STATUS[$liveStatus]}
                <span class="pill {STATUS[$liveStatus].tone}"><i></i>{STATUS[$liveStatus].label}</span>
              {/if}
            {/if}

            {#if isPlayingCurrent}
              <span
                class="signal {$streamGuard.health}"
                title={`Stream: ${HEALTH_LABEL[$streamGuard.health]} · ${$streamGuard.stalls} ${$streamGuard.stalls === 1 ? 'stall' : 'stalls'} in the last 2 minutes`}
                aria-label={`Stream ${HEALTH_LABEL[$streamGuard.health]}`}
              >
                <i></i><i></i><i></i>
              </span>
              <button class="ib" onclick={reloadStream} aria-label="Reload stream" data-tip="Reload stream · R">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 11a8 8 0 10-2.3 5.7"/><polyline points="20 4 20 11 13 11"/></svg>
              </button>
            {/if}

            <span class="spacer"></span>

            {#if isPlayingCurrent}
              <button class="ib" class:on={showTracks} onclick={() => (showTracks = !showTracks)} aria-label="Audio and subtitles" aria-expanded={showTracks} data-tip="Audio & subtitles · C">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="4.5" width="19" height="15" rx="2.5"/><path d="M7 11.5h4M13 11.5h4M7 15h7M16 15h1"/></svg>
              </button>
            {/if}
            <button class="ib" class:on={showKeys} onclick={() => (showKeys = !showKeys)} aria-label="Keyboard shortcuts" aria-expanded={showKeys} data-tip="Shortcuts · ?">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6.5 10h1M10.5 10h1M14.5 10h1M8 14h8"/></svg>
            </button>
            <button class="ib" class:on={theater} onclick={toggleTheater} aria-label="Theater mode" aria-pressed={theater} data-tip={theater ? 'Exit theater · T' : 'Theater · T'}>
              {#if theater}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="2.5" y="5" width="19" height="14" rx="2"/><rect x="9" y="9" width="10" height="7" rx="1" fill="currentColor"/></svg>
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="2.5" y="6.5" width="19" height="11" rx="2"/></svg>
              {/if}
            </button>
            <button class="ib" onclick={fullscreen} aria-label="Fullscreen" data-tip="Fullscreen · F">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/></svg>
            </button>
            {#if isPlayingCurrent}
              <button class="ib stop" onclick={() => stopLive()} aria-label="Stop" data-tip="Stop">
                <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>
              </button>
            {/if}
          </div>

          {#if showTracks && isPlayingCurrent}
            <div class="track-wrap" transition:jsFade={{ duration: 120 }}><TrackPanel /></div>
          {/if}
          {#if showKeys}
            <dl class="keys" transition:jsFade={{ duration: 120 }} aria-label="Keyboard shortcuts">
              <div><dt>↑ ↓</dt><dd>Move in list</dd></div>
              <div><dt>Enter</dt><dd>Watch</dd></div>
              <div><dt>PgUp PgDn</dt><dd>Previous / next channel</dd></div>
              <div><dt>Space</dt><dd>Pause</dd></div>
              <div><dt>+ −</dt><dd>Volume</dd></div>
              <div><dt>M</dt><dd>Mute</dd></div>
              <div><dt>L</dt><dd>Go live</dd></div>
              <div><dt>R</dt><dd>Reload stream</dd></div>
              <div><dt>T · Esc</dt><dd>Theater on / off</dd></div>
              <div><dt>F</dt><dd>Fullscreen</dd></div>
              <div><dt>C</dt><dd>Audio & subtitles</dd></div>
              <div><dt>[ ]</dt><dd>Sound earlier / later</dd></div>
              <div><dt>Ctrl K</dt><dd>Search</dd></div>
              <div><dt>G</dt><dd>Categories</dd></div>
            </dl>
          {/if}
        </div>

        <div class="info">
          <div class="info-main">
            <span class="info-logo">
              {#if current.logo_url}<img src={current.logo_url} alt="" />{:else}{current.name.trim().charAt(0).toUpperCase()}{/if}
            </span>
            <div class="info-text">
              <h2 class="ch-name" dir="auto">{current.name}</h2>
              <p class="ch-meta">
                {#if chNumber > 0}<span class="ch-num">CH {chNumber}</span>{/if}
                <span dir="auto">{current.group_name}</span>
              </p>
            </div>
          </div>
          <div class="info-actions">
            {#if isPlayingCurrent}
              <button
                class="chip"
                class:on-good={$streamGuard.stable}
                onclick={() => setStable(!$streamGuard.stable)}
                aria-pressed={$streamGuard.stable}
                title="Waits for a bigger buffer before resuming after a stall: fewer cuts, a few seconds more delay"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/>{#if $streamGuard.stable}<polyline points="8.5 12 11 14.5 15.5 9.5"/>{/if}</svg>
                Stable mode {$streamGuard.stable ? 'on' : 'off'}
              </button>
            {/if}
            <button class="chip" class:on={favIds.has(current.id)} onclick={(e) => toggleFav(current!, e.currentTarget)}>
              <svg viewBox="0 0 24 24" fill={favIds.has(current.id) ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2"><path d="M12 21s-7.5-4.6-9.6-9.4C.9 8.1 3.2 4.5 6.9 4.5c2.1 0 3.6 1.1 5.1 3 1.5-1.9 3-3 5.1-3 3.7 0 6 3.6 4.5 7.1C19.5 16.4 12 21 12 21z"/></svg>
              {favIds.has(current.id) ? 'In favorites' : 'Favorite'}
            </button>
            <button class="chip" onclick={() => setLiveMode($liveMode === 'app' ? 'window' : 'app')}>
              {#if $liveMode === 'app'}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M14 4h6v6"/><path d="M20 4l-8 8"/><path d="M19 14v4a2 2 0 01-2 2H6a2 2 0 01-2-2V7a2 2 0 012-2h4"/></svg>
                Pop out
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="4" width="19" height="14" rx="2"/><rect x="11" y="10" width="8" height="6" rx="1" fill="currentColor"/></svg>
                Play in app
              {/if}
            </button>
            <button class="chip" class:ok={probe[current.id] === 'ok'} class:bad={probe[current.id] === 'dead'} onclick={() => test(current!)} disabled={probe[current.id] === 'testing'}>
              {#if probe[current.id] === 'testing'}
                <span class="spinner small"></span>Checking…
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h4l3-8 4 16 3-8h6"/></svg>
                {probe[current.id] === 'ok' ? 'Stream is up' : probe[current.id] === 'dead' ? 'Not responding' : 'Check stream'}
              {/if}
            </button>
          </div>
        </div>

        {#if isPlayingCurrent && $streamGuard.phase !== 'ok'}
          <div class="guard {$streamGuard.phase}" role="status" aria-live="polite">
            {#if $streamGuard.phase === 'recovering'}
              <span class="mini-spin" aria-hidden="true"></span>
              <p><b>{$streamGuard.reason}.</b> Reconnecting, attempt {$streamGuard.attempt} of {$streamGuard.maxAttempts}{#if retryIn > 0} in {retryIn}s{/if}.</p>
              <button onclick={() => zap(1)}>Next channel</button>
            {:else if $streamGuard.phase === 'blocked'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><line x1="12" y1="7.5" x2="12" y2="12.5"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
              <p><b>{$streamGuard.reason}</b> Waiting {Math.floor(retryIn / 60)}:{String(retryIn % 60).padStart(2, '0')} before trying again, so the block doesn't get longer.</p>
              <button onclick={() => zap(1)}>Next channel</button>
            {:else}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>
              <p><b>This channel keeps dropping.</b> {$streamGuard.reason}. Gave up after {$streamGuard.maxAttempts} tries to avoid overloading the server.</p>
              <button class="primary" onclick={reloadStream}>Try again</button>
              <button onclick={() => zap(1)}>Next channel</button>
            {/if}
          </div>
        {:else if isPlayingCurrent && $streamGuard.notice}
          <div class="guard ok" role="status" transition:jsFade={{ duration: 150 }}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="5 12.5 10 17 19 7"/></svg>
            <p>{$streamGuard.notice}</p>
          </div>
        {/if}

        {#if isPlayingCurrent && !embedded}
          <p class="hint">Playing in a separate MPV window.</p>
        {:else if isPlayingCurrent && ($liveStatus === 'nosignal' || $liveStatus === 'offline')}
          {#if playlist && connectionsFull($accounts[playlist.id])}
            <p class="hint">
              All {$accounts[playlist.id]?.max_connections} connections on this subscription are in use, so the server is refusing new streams.
              Close other apps or devices playing from this account.
            </p>
          {:else}
            <p class="hint">The server isn't sending video for this channel right now. Try the next channel with PgDn.</p>
          {/if}
        {/if}
        {#if probe[current.id] === 'dead'}
          <p class="hint">The stream didn't answer. It may be offline, or the server is rate-limiting this connection.</p>
        {/if}
        {#if $liveError}
          <p class="hint error">{$liveError}</p>
        {/if}

        {#if strip.length > 1}
          <section class="strip" aria-label={`More in ${current.group_name}`}>
            <header class="strip-head">
              <h3 dir="auto">More in {current.group_name || 'this category'}</h3>
              <button onclick={() => openView(current!.group_name)}>See all</button>
            </header>
            <div class="strip-track">
              {#each strip as ch, i (ch.id)}
                <button
                  class="tile"
                  class:playing={playingId === ch.id}
                  onclick={() => play(ch)}
                  aria-label={`Watch ${ch.name}`}
                >
                  <span class="tile-art">
                    {#if ch.logo_url}<img src={ch.logo_url} alt="" loading="lazy" />{:else}<span>{ch.name.trim().charAt(0).toUpperCase()}</span>{/if}
                    {#if playingId === ch.id}<span class="tile-on"><i></i>On now</span>{/if}
                  </span>
                  <span class="tile-name" dir="auto"><b>{i + 1}</b> {ch.name}</span>
                </button>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    {:else}
      <div class="stage-empty">
        <span class="kicker"><b>J</b>LIVE TV</span>
        <h2>Pick a channel</h2>
        <p>Choose a category on the left, or search {nf.format(totalLive)} channels with Ctrl K. Playback keeps going in a mini player while you browse the rest of the app.</p>
        {#if !$playlists.length}
          <button class="btn-white" onclick={() => goto('/playlists')}>Add a playlist</button>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  .live-page {
    margin: -24px;
    height: 100vh;
    display: grid;
    grid-template-columns: minmax(330px, 400px) minmax(0, 1fr);
    overflow: hidden;
  }

  /* ── Guide ──────────────────────────────────────────────────────────── */

  .guide {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: oklch(var(--n135));
    box-shadow: 1px 0 0 oklch(var(--n1000) / 0.06);
  }

  .guide-head {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 24px 16px 6px;
  }
  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 4px;
  }
  .title-row h1 {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 1.625rem;
    font-weight: 900;
    letter-spacing: -0.03em;
  }
  .live-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 0 4px oklch(0.58 0.225 27 / 0.25);
  }
  .pl-name {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .pl-select {
    max-width: 170px;
    height: 32px;
    padding: 0 10px;
    font-size: 0.8125rem;
    font-weight: 600;
    background: oklch(var(--n1000) / 0.06);
    border-color: oklch(var(--n1000) / 0.1);
  }

  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex-shrink: 0;
    height: 44px;
    padding: 0 10px 0 42px;
    border-radius: 10px;
    background: oklch(var(--n1000) / 0.06);
    box-shadow: inset 0 0 0 1px oklch(var(--n1000) / 0.08);
    transition: box-shadow 150ms var(--ease-out), background 150ms var(--ease-out);
  }
  .search:focus-within { background: oklch(var(--n110)); box-shadow: inset 0 0 0 2px var(--color-text); }
  .search > :global(svg) { position: absolute; left: 14px; width: 18px; height: 18px; color: var(--color-text-muted); }
  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    background: none;
    font-size: 0.9375rem;
  }
  .search input::-webkit-search-cancel-button { display: none; }
  kbd {
    flex-shrink: 0;
    padding: 2px 7px;
    border-radius: 4px;
    background: oklch(var(--n1000) / 0.08);
    color: var(--color-text-muted);
    font-family: inherit;
    font-size: 0.6875rem;
    font-weight: 700;
  }
  .clear {
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(var(--n1000) / 0.1);
    color: var(--color-text);
  }
  .clear :global(svg) { width: 13px; height: 13px; }

  /* Opens the categories drawer; shows which one you're in */
  .cats-btn {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 54px;
    padding: 0 12px 0 14px;
    border-radius: 10px;
    background: oklch(var(--n1000) / 0.04);
    box-shadow: inset 0 0 0 1px oklch(var(--n1000) / 0.08);
    color: var(--color-text);
    text-align: start;
    transition: background 150ms var(--ease-out), box-shadow 150ms var(--ease-out);
  }
  .cats-btn > :global(svg) { width: 20px; height: 20px; flex-shrink: 0; color: var(--color-text-muted); }
  .cats-btn:hover { background: oklch(var(--n1000) / 0.08); }
  .cats-btn:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .cats-btn.on { box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.6); background: oklch(0.58 0.225 27 / 0.1); }
  .cats-btn.on > :global(svg) { color: var(--color-accent-soft); }
  .cats-btn-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .cats-btn-text small {
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .cats-btn-text b {
    font-size: 0.9375rem;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tabs { display: flex; flex-wrap: wrap; gap: 6px; }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    max-width: 100%;
    height: 32px;
    padding: 0 12px;
    border-radius: 16px;
    background: oklch(var(--n1000) / 0.06);
    color: oklch(var(--n850));
    font-size: 0.8125rem;
    font-weight: 600;
    white-space: nowrap;
    transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
  }
  .tab > :global(svg) { width: 14px; height: 14px; flex-shrink: 0; color: var(--color-accent); }
  .tab > span:last-child { color: var(--color-text-muted); font-size: 0.75rem; font-variant-numeric: tabular-nums; }
  .tab-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .tab:hover { background: oklch(var(--n1000) / 0.11); color: var(--color-text); }
  .tab:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .tab.on { background: var(--color-text); color: oklch(var(--n140)); font-weight: 700; }
  .tab.on > span:last-child { color: oklch(var(--n400)); }
  .cat-dot { flex: none; width: 8px; height: 8px; border-radius: 50%; }

  .fav-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 12px 6px;
    background: oklch(var(--n135));
    font-size: 0.6875rem;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .fav-head button { background: none; color: var(--color-text); font: inherit; letter-spacing: inherit; text-transform: none; font-size: 0.8125rem; }
  .fav-head button:hover { text-decoration: underline; text-underline-offset: 3px; }

  /* ── Categories drawer ──────────────────────────────────────────────── */

  .drawer {
    position: absolute;
    inset: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    background: oklch(var(--n155));
    box-shadow: 1px 0 0 oklch(var(--n1000) / 0.08);
    /* Starts off-canvas; slid from script (see DRAWER_MS) */
    transform: translate3d(-101%, 0, 0);
    will-change: transform;
  }
  .drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 22px 14px 12px 20px;
  }
  .drawer-head h2 { font-size: 1.25rem; font-weight: 800; letter-spacing: -0.02em; }
  .drawer-head h2 span { margin-left: 6px; font-size: 0.875rem; font-weight: 600; color: var(--color-text-muted); }
  .drawer-x {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: none;
    color: var(--color-text-muted);
  }
  .drawer-x :global(svg) { width: 16px; height: 16px; }
  .drawer-x:hover { background: oklch(var(--n1000) / 0.08); color: var(--color-text); }
  .drawer-search { margin: 0 16px 10px; }

  .cat-list {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 10px 24px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .cat {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 42px;
    padding: 0 12px;
    border-radius: 8px;
    background: none;
    color: oklch(var(--n840));
    font-size: 0.875rem;
    font-weight: 500;
    text-align: start;
    transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
  }
  .cat:hover { background: oklch(var(--n1000) / 0.06); color: var(--color-text); }
  .cat:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .cat.on { background: oklch(0.58 0.225 27 / 0.18); color: var(--color-text); font-weight: 700; }
  .cat-name { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cat-count {
    flex-shrink: 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .cat-none { padding: 24px 12px; color: var(--color-text-muted); font-size: 0.875rem; }

  /* ── Channels ───────────────────────────────────────────────────────── */

  .list-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 20px 8px;
  }
  .list-head h2 {
    font-size: 1rem;
    font-weight: 800;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .list-head span {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .list {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 8px 24px;
    outline: none;
  }

  .row {
    position: relative;
    display: flex;
    align-items: center;
    border-radius: 8px;
    transition: background 120ms var(--ease-out);
  }
  .row:hover { background: oklch(var(--n1000) / 0.045); }
  .row.hl { background: oklch(var(--n1000) / 0.09); }
  .row.playing { background: oklch(0.58 0.225 27 / 0.16); box-shadow: inset 3px 0 0 var(--color-accent); }
  .row.playing.hl { background: oklch(0.58 0.225 27 / 0.24); }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 62px;
    padding: 0 6px 0 8px;
    background: none;
    color: var(--color-text);
    text-align: start;
    border-radius: 8px;
  }
  .row-main:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }

  .num {
    width: 30px;
    flex-shrink: 0;
    text-align: end;
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .playing .num { color: var(--color-accent-soft); }

  .logo {
    width: 64px;
    height: 42px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: 6px;
    background: oklch(var(--n240));
    overflow: hidden;
  }
  .logo img { width: 100%; height: 100%; object-fit: contain; padding: 5px; }
  .logo-letter { font-size: 1.125rem; font-weight: 800; color: var(--color-text-muted); }

  .row-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .row-name {
    font-size: 0.9375rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .playing .row-name { font-weight: 800; }
  .live-sub { display: inline-flex; align-items: center; gap: 6px; color: var(--color-accent-soft) !important; font-weight: 700; }
  .live-sub i { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .row-sub {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .eq { display: flex; align-items: flex-end; gap: 2px; height: 14px; margin-right: 4px; }
  .eq i { width: 3px; background: var(--color-accent); border-radius: 1px; animation: eq 900ms ease-in-out infinite; }
  .eq i:nth-child(2) { animation-delay: -300ms; }
  .eq i:nth-child(3) { animation-delay: -600ms; }
  @keyframes eq { 0%, 100% { height: 4px; } 50% { height: 14px; } }

  .row-fav {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    margin-right: 6px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: none;
    color: var(--color-text-muted);
    /* Shown instantly (no opacity transition: WebKitGTK blinks a frame when one ends);
       opacity rather than visibility keeps it reachable with Tab */
    opacity: 0;
    transition: color 120ms var(--ease-out), background 120ms var(--ease-out);
  }
  .row-fav :global(svg) { width: 18px; height: 18px; }
  .row:hover .row-fav, .row.hl .row-fav, .row-fav.on, .row-fav:focus-visible { opacity: 1; }
  .row-fav:hover { background: oklch(var(--n1000) / 0.08); color: var(--color-text); }
  .row-fav.on { color: var(--color-accent); }

  .empty {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 40px 16px;
    color: var(--color-text-muted);
    font-size: 0.875rem;
    line-height: 1.5;
  }
  .empty strong { color: var(--color-text); font-size: 1rem; }

  .more { display: grid; place-items: center; height: 60px; }

  /* ── Stage ──────────────────────────────────────────────────────────── */

  .live-page.theater { grid-template-columns: 0 minmax(0, 1fr); }
  .live-page.theater .stage-body { margin-inline: auto; align-items: center; }
  .live-page.theater .player,
  .live-page.theater .info,
  .live-page.theater .strip { width: min(100%, calc((100vh - 200px) * 16 / 9)); }
  .live-page.theater .guide { visibility: hidden; }

  .stage {
    position: relative;
    min-height: 0;
    overflow: hidden auto;
    isolation: isolate;
    background: radial-gradient(90% 60% at 40% 0%, oklch(var(--n200)), var(--color-base) 70%);
  }

  /* Ambilight: the channel logo, blurred, glowing out from behind the screen */
  .glow {
    position: absolute;
    inset: -6% -4%;
    z-index: -1;
    pointer-events: none;
  }
  .glow img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(90px) saturate(1.7) brightness(0.75);
    opacity: 0.45;
  }

  .stage-body {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: clamp(18px, 2vw, 32px);
    max-width: 1600px;
  }

  /* Player: screen + attached control bar */
  .player {
    position: relative;
    width: min(100%, calc((100vh - 330px) * 16 / 9));
    border-radius: 12px;
    background: oklch(var(--n130));
    box-shadow: 0 40px 80px -30px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(var(--n1000) / 0.07);
  }

  .screen {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    display: grid;
    place-items: center;
    border-radius: 12px 12px 0 0;
    background: radial-gradient(80% 80% at 50% 40%, oklch(var(--n280)), oklch(var(--n150)));
    overflow: hidden;
  }
  .screen.video { background: oklch(var(--n80)); }
  .screen.video > * { visibility: hidden; }
  .screen-logo {
    max-width: 44%;
    max-height: 44%;
    object-fit: contain;
    filter: drop-shadow(0 12px 30px oklch(0 0 0 / 0.5));
  }
  .screen-letter { font-size: 5rem; font-weight: 900; color: oklch(var(--n1000) / 0.15); }
  /* The whole screen starts the channel; the button says so */
  .screen-play {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    padding: 22px;
    background: linear-gradient(to top, oklch(0 0 0 / 0.55), transparent 45%);
  }
  .screen-play:focus-visible { outline: none; }
  .watch-cta {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 48px;
    padding: 0 12px 0 20px;
    border-radius: 24px;
    background: var(--color-text);
    color: oklch(var(--n140));
    font-size: 1rem;
    font-weight: 800;
    box-shadow: 0 10px 30px oklch(0 0 0 / 0.45);
    transition: background 150ms var(--ease-out);
  }
  .watch-cta :global(svg) { width: 20px; height: 20px; }
  .watch-cta kbd { background: oklch(0 0 0 / 0.1); color: oklch(var(--n350)); }
  .screen-play:hover .watch-cta { background: oklch(var(--n860)); }
  .screen-play:focus-visible .watch-cta { outline: 2px solid var(--color-text); outline-offset: 3px; }

  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 56px;
    padding: 0 10px;
    border-radius: 0 0 12px 12px;
    background: oklch(var(--n160));
    box-shadow: inset 0 1px 0 oklch(var(--n1000) / 0.06);
  }
  .spacer { flex: 1; }
  .channels-btn {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    margin-right: 8px;
    padding: 0 14px 0 11px;
    border-radius: 18px;
    background: var(--color-text);
    color: oklch(var(--n140));
    font-size: 0.8125rem;
    font-weight: 800;
    transition: background 150ms var(--ease-out);
  }
  .channels-btn :global(svg) { width: 17px; height: 17px; }
  .channels-btn:hover { background: oklch(var(--n860)); }
  .channels-btn:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .ib {
    position: relative;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: none;
    color: oklch(var(--n900));
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .ib > :global(svg) { width: 22px; height: 22px; }
  .ib:hover { background: oklch(var(--n1000) / 0.08); color: var(--color-text); }
  .ib:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .ib.on { color: var(--color-text); background: oklch(var(--n1000) / 0.1); }
  .ib.stop:hover { color: var(--color-accent-soft); }

  /* Tooltips open downward: anything above the bar sits under the native video */
  .ib[data-tip]::after {
    content: attr(data-tip);
    position: absolute;
    top: calc(100% + 8px);
    left: 50%;
    z-index: 5;
    transform: translate(-50%, 0);
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--color-text);
    color: oklch(var(--n140));
    font-size: 0.75rem;
    font-weight: 700;
    white-space: nowrap;
    /* Instant show/hide: see .row-fav */
    visibility: hidden;
    pointer-events: none;
  }
  .ib:hover::after,
  .ib:focus-visible::after { visibility: visible; }

  .vol {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0 6px 0 2px;
  }
  .vol-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 96px;
    height: 4px;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: linear-gradient(to right, var(--color-text) var(--fill), oklch(var(--n1000) / 0.2) var(--fill));
    cursor: pointer;
  }
  .vol-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--color-text);
    box-shadow: 0 1px 4px oklch(0 0 0 / 0.5);
  }
  .vol-slider:focus-visible { outline: 2px solid var(--color-text); outline-offset: 6px; }
  .vol-value {
    min-width: 42px;
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .signal {
    flex-shrink: 0;
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    height: 16px;
    margin: 0 4px 0 8px;
  }
  .signal i { width: 4px; border-radius: 1px; background: oklch(var(--n1000) / 0.2); }
  .signal i:nth-child(1) { height: 6px; }
  .signal i:nth-child(2) { height: 11px; }
  .signal i:nth-child(3) { height: 16px; }
  .signal.good i { background: var(--color-accent-green); }
  .signal.fair i:nth-child(-n + 2) { background: var(--color-accent-yellow); }
  .signal.poor i:nth-child(1) { background: var(--color-accent); }

  .mini-spin {
    flex-shrink: 0;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid oklch(var(--n1000) / 0.2);
    border-top-color: currentColor;
    animation: spin 800ms linear infinite;
  }

  .guard {
    display: flex;
    align-items: center;
    gap: 12px;
    width: min(100%, calc((100vh - 330px) * 16 / 9));
    padding: 12px 14px;
    border-radius: 10px;
    font-size: 0.875rem;
    line-height: 1.45;
    background: oklch(0.78 0.15 75 / 0.12);
    box-shadow: inset 0 0 0 1px oklch(0.78 0.15 75 / 0.35);
    color: var(--color-text);
  }
  .guard p { flex: 1; min-width: 0; color: oklch(var(--n880)); }
  .guard p b { color: var(--color-text); }
  .guard > :global(svg) { width: 20px; height: 20px; flex-shrink: 0; }
  .guard .mini-spin { width: 18px; height: 18px; color: var(--color-accent-yellow); }
  .guard.recovering { color: var(--color-accent-yellow); }
  .guard.blocked, .guard.failed { background: oklch(0.58 0.225 27 / 0.14); box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.4); }
  .guard.blocked > :global(svg), .guard.failed > :global(svg) { color: var(--color-accent-soft); }
  .guard.ok { background: oklch(0.76 0.13 165 / 0.12); box-shadow: inset 0 0 0 1px oklch(0.76 0.13 165 / 0.35); }
  .guard.ok > :global(svg) { color: var(--color-accent-green); }
  .guard button {
    flex-shrink: 0;
    height: 32px;
    padding: 0 14px;
    border-radius: 6px;
    background: oklch(var(--n1000) / 0.1);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
  }
  .guard button:hover { background: oklch(var(--n1000) / 0.18); }
  .guard button.primary { background: var(--color-text); color: oklch(var(--n140)); }
  .chip.on-good { color: var(--color-accent-green); }

  .pill {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0 11px;
    border-radius: 14px;
    font-size: 0.75rem;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    background: oklch(var(--n1000) / 0.08);
    color: var(--color-text);
  }
  .pill i { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
  .pill.live i { background: oklch(0.72 0.19 150); box-shadow: 0 0 0 3px oklch(0.72 0.19 150 / 0.25); }
  .pill.wait { color: var(--color-accent-yellow); }
  .pill.bad { color: var(--color-accent-soft); }
  .pill.idle { color: var(--color-text-muted); }
  .pill.behind {
    background: var(--color-accent);
    color: var(--color-on-accent);
    transition: background 150ms var(--ease-out);
  }
  .pill.behind i { animation: pulse 1.6s ease-in-out infinite; }
  .pill.behind small { font-size: 0.6875rem; opacity: 0.85; letter-spacing: 0; font-variant-numeric: tabular-nums; }
  .pill.behind:hover { background: var(--color-accent-hover); }
  @keyframes pulse { 50% { opacity: 0.35; } }

  .track-wrap { padding: 0 10px 6px; border-top: 1px solid oklch(var(--n1000) / 0.06); }

  .keys {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 10px 20px;
    padding: 16px 18px;
    border-top: 1px solid oklch(var(--n1000) / 0.06);
    font-size: 0.8125rem;
    color: var(--color-text-muted);
  }
  .keys div { display: flex; align-items: center; gap: 9px; }
  .keys dt {
    padding: 2px 7px;
    border-radius: 4px;
    background: oklch(var(--n1000) / 0.08);
    color: var(--color-text);
    font-size: 0.6875rem;
    font-weight: 700;
    white-space: nowrap;
  }

  /* Channel info */
  .info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 14px 24px;
    width: min(100%, calc((100vh - 330px) * 16 / 9));
  }
  .info-main { display: flex; align-items: center; gap: 14px; min-width: 0; }
  .info-logo {
    flex-shrink: 0;
    width: 64px;
    height: 64px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    background: oklch(var(--n240));
    font-size: 1.5rem;
    font-weight: 800;
    color: var(--color-text-muted);
    overflow: hidden;
  }
  .info-logo img { width: 100%; height: 100%; object-fit: contain; padding: 8px; }
  .info-text { min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .ch-name {
    font-size: clamp(1.375rem, 2vw, 1.875rem);
    font-weight: 900;
    line-height: 1.12;
    letter-spacing: -0.025em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ch-name:dir(rtl) { letter-spacing: 0; line-height: 1.35; }
  .ch-meta {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 0.875rem;
    color: var(--color-text-muted);
    min-width: 0;
  }
  .ch-meta span:last-child { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ch-num {
    flex-shrink: 0;
    padding: 1px 7px;
    border-radius: 4px;
    background: oklch(var(--n1000) / 0.1);
    color: var(--color-text);
    font-size: 0.75rem;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .info-actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 15px 0 12px;
    border-radius: 19px;
    background: oklch(var(--n1000) / 0.08);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
    white-space: nowrap;
    transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
  }
  .chip :global(svg) { width: 17px; height: 17px; flex-shrink: 0; }
  .chip:hover:not(:disabled) { background: oklch(var(--n1000) / 0.15); }
  .chip:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .chip.on { color: var(--color-accent-soft); }
  .chip.ok { color: var(--color-accent-green); }
  .chip.bad { color: var(--color-accent-soft); }

  .hint { font-size: 0.875rem; color: oklch(var(--n850)); margin-top: -6px; }
  .hint.error { color: var(--color-accent-soft); }

  /* More in category */
  .strip { display: flex; flex-direction: column; gap: 10px; margin-top: 6px; width: min(100%, calc((100vh - 330px) * 16 / 9)); }
  .strip-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .strip-head h3 {
    font-size: 1rem;
    font-weight: 800;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .strip-head button {
    flex-shrink: 0;
    background: none;
    color: var(--color-text-muted);
    font-size: 0.8125rem;
    font-weight: 700;
  }
  .strip-head button:hover { color: var(--color-text); }
  .strip-track {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 168px;
    gap: 10px;
    overflow-x: auto;
    padding: 4px 2px 12px;
    scrollbar-width: thin;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 0;
    background: none;
    color: var(--color-text);
    text-align: start;
    border-radius: 8px;
  }
  .tile-art {
    position: relative;
    display: grid;
    place-items: center;
    aspect-ratio: 16 / 9;
    border-radius: 8px;
    background: oklch(var(--n220));
    box-shadow: inset 0 0 0 1px oklch(var(--n1000) / 0.06);
    overflow: hidden;
    /* Paint-only lift (top, not transform): WebKitGTK blinks when a GPU transition ends */
    top: 0;
    transition: top 200ms var(--ease-out), box-shadow 200ms var(--ease-out);
  }
  .tile-art img { width: 100%; height: 100%; object-fit: contain; padding: 14px 22px; }
  .tile-art > span:not(.tile-on) { font-size: 1.5rem; font-weight: 800; color: var(--color-text-muted); }
  .tile:hover .tile-art { top: -3px; box-shadow: inset 0 0 0 1px oklch(var(--n1000) / 0.2), 0 14px 28px -12px oklch(0 0 0 / 0.8); }
  .tile:focus-visible { outline: none; }
  .tile:focus-visible .tile-art { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .tile.playing .tile-art { box-shadow: inset 0 0 0 2px var(--color-accent); }
  .tile-on {
    position: absolute;
    top: 6px;
    left: 6px;
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
  .tile-on i { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }
  .tile-name {
    font-size: 0.8125rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tile-name b { color: var(--color-text-muted); font-weight: 700; margin-right: 4px; font-variant-numeric: tabular-nums; }
  .tile.playing .tile-name { color: var(--color-accent-soft); }

  /* Now-playing shortcut at the top of the list */
  .np-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0 16px 10px;
    padding: 10px 12px;
    border-radius: 8px;
    background: oklch(0.58 0.225 27 / 0.16);
    box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.35);
    color: var(--color-text);
    text-align: start;
    transition: background 150ms var(--ease-out);
  }
  .np-row:hover { background: oklch(0.58 0.225 27 / 0.26); }
  .np-row > :global(svg) { width: 16px; height: 16px; flex-shrink: 0; color: var(--color-text-muted); }
  .np-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .np-label { font-size: 0.6875rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--color-accent-soft); }
  .np-name { font-size: 0.875rem; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .kicker {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.3em;
  }
  .kicker b { font-size: 1.25rem; font-weight: 900; letter-spacing: -0.04em; color: var(--color-accent); }

  .btn-white {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 48px;
    padding: 0 26px;
    border-radius: 6px;
    background: var(--color-text);
    color: oklch(var(--n140));
    font-size: 1rem;
    font-weight: 700;
    transition: background 150ms var(--ease-out);
  }
  .btn-white:hover { background: oklch(var(--n850)); }
  .btn-white:focus-visible { outline: 2px solid var(--color-text); outline-offset: 3px; }

  .stage-empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: flex-start;
    gap: 12px;
    padding: clamp(28px, 4vw, 56px);
    max-width: 620px;
    background: radial-gradient(70% 60% at 80% 20%, oklch(0.4 0.15 27 / 0.25), transparent 70%);
  }
  .stage-empty h2 { font-size: 2.5rem; font-weight: 900; letter-spacing: -0.03em; }
  .stage-empty p { color: var(--color-text-muted); line-height: 1.6; }

  /* ── Shared ─────────────────────────────────────────────────────────── */

  .spinner {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 3px solid oklch(var(--n1000) / 0.12);
    border-top-color: var(--color-accent);
    animation: spin 800ms linear infinite;
  }
  .spinner.small { width: 18px; height: 18px; border-width: 2.5px; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .sk-row { gap: 12px; height: 62px; padding: 0 8px 0 46px; }
  .sk {
    border-radius: 6px;
    background: linear-gradient(90deg, oklch(var(--n1000) / 0.05) 0%, oklch(var(--n1000) / 0.1) 50%, oklch(var(--n1000) / 0.05) 100%);
    background-size: 200% 100%;
    /* Hidden for the first 160ms: fast loads go straight to content instead of
       flashing a skeleton for a frame or two */
    visibility: hidden;
    animation: shimmer 1.6s linear infinite, sk-show 0s 160ms forwards;
  }
  @keyframes sk-show { to { visibility: visible; } }
  .sk-logo { width: 64px; height: 42px; flex-shrink: 0; }
  .sk-text { height: 14px; width: 60%; }
  @keyframes shimmer { from { background-position: 100% 0; } to { background-position: -100% 0; } }

  @media (max-width: 1150px) {
    .live-page { grid-template-columns: 320px minmax(0, 1fr); }
  }

  @media (prefers-reduced-motion: reduce) {
    .eq i, .pill.behind i, .sk, .spinner { animation: none; visibility: visible; }
  }
</style>
