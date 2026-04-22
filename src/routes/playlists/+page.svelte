<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Hls from 'hls.js';
  import mpegts from 'mpegts.js';
  import PlaylistItem from '$lib/components/PlaylistItem.svelte';
  import AddPlaylistModal from '$lib/components/AddPlaylistModal.svelte';
  import { deletePlaylist, exportPlaylist, refreshPlaylist, getGroupsByType, getRecentlyAdded, getChannelsByGroup, getSeriesInfo, getVodInfo, searchChannelsInPlaylist, recordViewing, detectExternalPlayers, launchExternalPlayer, addFavorite, removeFavorite, isFavorite, mpvPlay, mpvLoad, mpvStop, mpvPause, mpvFullscreen, mpvIsRunning, startDownload, getSetting, setSetting } from '$lib/tauri';
  import type { Channel, ChannelGroup, SeriesDetail, VodDetail, ExternalPlayer } from '$lib/tauri';
  import {
    playlists, selectedPlaylist, contentTypeCounts,
    loadPlaylists, loadContentTypeCounts, browsingState,
  } from '$lib/stores/playlists';
  import type { PlaylistBrowsingState, TabState } from '$lib/stores/playlists';
  import { get } from 'svelte/store';
  import { downloads } from '$lib/stores/downloads';

  // Map channel_id → download progress for inline indicators
  let dlProgressMap = $derived((() => {
    const map = new Map<number, { progress: number; status: string }>();
    for (const d of $downloads) {
      if (d.channel_id && (d.status === 'downloading' || d.status === 'queued' || d.status === 'paused')) {
        map.set(d.channel_id, { progress: d.progress, status: d.status });
      }
    }
    return map;
  })());

  let showAddModal = $state(false);
  let actionLoading = $state(false);

  // Content type tabs
  let activeTab = $state<'live' | 'vod' | 'series'>('live');
  let tabGroups = $state<ChannelGroup[]>([]);
  let tabGroupsLoading = $state(false);

  // Selected group & channels
  let selectedGroup = $state<string | null>(null);
  let groupChannels = $state<Channel[]>([]);
  let channelsLoading = $state(false);
  let channelOffset = $state(0);
  let hasMore = $state(false);
  const PAGE_SIZE = 80;

  // Inline player
  let playingChannel = $state<Channel | null>(null);
  let playStartTime = $state<number | null>(null);
  let videoEl = $state<HTMLVideoElement | undefined>();
  let playerContainerEl = $state<HTMLDivElement | undefined>();
  let hls: Hls | null = null;
  let mpegtsPlayer: mpegts.Player | null = null;
  let playerLoading = $state(false);
  let playerPaused = $state(false);
  let playerError = $state('');
  let isFullscreen = $state(false);
  // Series detail
  let seriesDetail = $state<SeriesDetail | null>(null);
  let seriesLoading = $state(false);
  let selectedSeason = $state<string | null>(null);
  let viewingSeries = $state<Channel | null>(null);
  // VOD detail
  let vodDetail = $state<VodDetail | null>(null);
  let vodLoading = $state(false);
  let vodError = $state('');
  let viewingVod = $state<Channel | null>(null);

  let retryCount = 0;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  const MAX_RETRIES = 8;
  let hlsRecoveryCount = 0;

  type StreamType = 'hls' | 'ts' | 'direct';

  // External player for VOD/series
  let externalPlayers = $state<ExternalPlayer[]>([]);
  let vodPlayingExternal = $state(false);

  // Stall watchdog — detects frozen video and auto-recovers
  let stallWatchdog: ReturnType<typeof setInterval> | null = null;
  let lastPlayPos = 0;
  let stallCount = 0;
  let currentStreamUrl = '';

  // Per-tab state cache: remembers group/channels/series for each tab
  let tabCache: Record<string, TabState> = {};

  function saveCurrentTabState() {
    tabCache[activeTab] = {
      tabGroups,
      selectedGroup,
      groupChannels,
      channelOffset,
      hasMore,
      viewingSeries,
      seriesDetail,
      selectedSeason,
      viewingVod,
      vodDetail,
    };
  }

  function restoreTabState(tab: 'live' | 'vod' | 'series'): boolean {
    const cached = tabCache[tab];
    if (!cached) return false;
    tabGroups = cached.tabGroups;
    selectedGroup = cached.selectedGroup;
    groupChannels = cached.groupChannels;
    channelOffset = cached.channelOffset;
    hasMore = cached.hasMore;
    viewingSeries = cached.viewingSeries;
    seriesDetail = cached.seriesDetail;
    selectedSeason = cached.selectedSeason;
    viewingVod = cached.viewingVod;
    vodDetail = cached.vodDetail;
    return true;
  }

  /** Extensions that browsers CAN play inline */
  const BROWSER_PLAYABLE = ['.mp4', '.webm', '.ogg'];

  function detectStreamType(url: string): StreamType {
    const lower = url.toLowerCase();
    if (lower.endsWith('.m3u8') || lower.includes('.m3u8')) return 'hls';
    if (lower.endsWith('.ts')) return 'ts';
    return 'direct';
  }

  function isBrowserPlayable(url: string): boolean {
    const lower = url.toLowerCase();
    return BROWSER_PLAYABLE.some(ext => lower.endsWith(ext));
  }

  function setupStream(url: string) {
    if (!videoEl) return;
    destroyPlayer();
    playerLoading = true;
    playerError = '';
    playerPaused = false;
    vodPlayingExternal = false;
    retryCount = 0;
    hlsRecoveryCount = 0;
    stallCount = 0;
    lastPlayPos = 0;
    currentStreamUrl = url;

    const type = detectStreamType(url);
    if (type === 'ts' && mpegts.isSupported()) {
      setupMpegts(url);
    } else if (type === 'hls' && Hls.isSupported()) {
      setupHls(url);
    } else if (type === 'hls' && videoEl.canPlayType('application/vnd.apple.mpegurl')) {
      videoEl.src = url; videoEl.play().catch(() => {});
    } else if (type === 'direct' && isBrowserPlayable(url)) {
      videoEl.src = url; videoEl.play().catch(() => {});
    } else if (type === 'direct') {
      openInExternalPlayer(url);
    } else {
      videoEl.src = url; videoEl.play().catch(() => {});
    }
    // Start stall watchdog for live streams
    if (type !== 'direct') startStallWatchdog();
  }

  /**
   * STALL WATCHDOG: runs every 3s, checks if video is actually playing.
   * - If frozen for 6s → seeks to live edge (HLS) or reconnects (mpegts)
   * - If buffer < 5s ahead → auto-downgrades quality
   * This prevents visible cuts and infinite loading.
   */
  function startStallWatchdog() {
    stopStallWatchdog();
    stallWatchdog = setInterval(() => {
      if (!videoEl || playerPaused || vodPlayingExternal) return;
      const currentPos = videoEl.currentTime;
      const buffered = videoEl.buffered;
      const bufferEnd = buffered.length > 0 ? buffered.end(buffered.length - 1) : 0;
      const bufferAhead = bufferEnd - currentPos;

      if (currentPos === lastPlayPos && !videoEl.paused && !videoEl.ended) {
        stallCount++;
        if (stallCount >= 2) {
          stallCount = 0;
          if (hls) {
            // Seek to live edge to unstick
            if (hls.liveSyncPosition && hls.liveSyncPosition > currentPos + 2) {
              videoEl.currentTime = hls.liveSyncPosition;
            } else if (bufferEnd > currentPos + 1) {
              videoEl.currentTime = bufferEnd - 0.5;
            }
            hls.startLoad();
            videoEl.play().catch(() => {});
          } else if (mpegtsPlayer) {
            setupMpegts(currentStreamUrl);
          } else {
            videoEl.play().catch(() => {});
          }
        }
      } else {
        stallCount = 0;
        if (playerError && currentPos > lastPlayPos) playerError = '';
      }
      lastPlayPos = currentPos;

      // Auto-downgrade quality when buffer is dangerously low
      if (hls && bufferAhead < 5 && bufferAhead > 0 && hls.currentLevel > 0) {
        hls.nextLevel = Math.max(0, hls.currentLevel - 1);
      }
    }, 3000);
  }

  function stopStallWatchdog() {
    if (stallWatchdog) { clearInterval(stallWatchdog); stallWatchdog = null; }
  }

  async function openInExternalPlayer(url: string) {
    playerLoading = false;
    vodPlayingExternal = true;

    if (externalPlayers.length === 0) {
      playerError = 'No external player found. Install VLC or MPV.';
      vodPlayingExternal = false;
      return;
    }

    try {
      await launchExternalPlayer(externalPlayers[0].path, url);
    } catch (e) {
      playerError = `Failed to open ${externalPlayers[0].name}: ${e}`;
      vodPlayingExternal = false;
    }
  }

  async function openCurrentInPlayer(playerIdx: number = 0) {
    if (!playingChannel) return;
    const p = externalPlayers[playerIdx];
    if (!p) return;
    try {
      await launchExternalPlayer(p.path, playingChannel.stream_url);
      vodPlayingExternal = true;
      playerError = '';
    } catch (e) {
      playerError = `Failed to open ${p.name}: ${e}`;
    }
  }

  function setupHls(url: string) {
    if (!videoEl) return;
    hls = new Hls({
      enableWorker: true,
      lowLatencyMode: false,
      // BUFFER: huge = no cuts ever
      maxBufferLength: 120,
      maxMaxBufferLength: 300,
      maxBufferSize: 200_000_000,
      maxBufferHole: 0.5,
      backBufferLength: 90,
      // LIVE EDGE: safe distance behind
      liveSyncDurationCount: 4,
      liveMaxLatencyDurationCount: 15,
      liveDurationInfinity: true,
      liveBackBufferLength: 90,
      // ABR: conservative start, ramps up fast
      startLevel: -1,
      abrEwmaDefaultEstimate: 3_000_000,
      abrEwmaFastLive: 3,
      abrEwmaSlowLive: 9,
      abrBandWidthUpFactor: 0.7,
      abrBandWidthFactor: 0.9,
      abrMaxWithRealBitrate: true,
      // FRAGMENT: aggressive prefetch
      maxFragLookUpTolerance: 0.25,
      startFragPrefetch: true,
      // NETWORK: maximum resilience — 20 retries, long timeouts
      manifestLoadingTimeOut: 30000,
      manifestLoadingMaxRetry: 20,
      manifestLoadingRetryDelay: 500,
      manifestLoadingMaxRetryTimeout: 60000,
      levelLoadingTimeOut: 30000,
      levelLoadingMaxRetry: 20,
      levelLoadingRetryDelay: 500,
      levelLoadingMaxRetryTimeout: 60000,
      fragLoadingTimeOut: 40000,
      fragLoadingMaxRetry: 20,
      fragLoadingRetryDelay: 500,
      fragLoadingMaxRetryTimeout: 60000,
      // PERF
      testBandwidth: true,
      progressive: true,
      capLevelOnFPSDrop: true,
      capLevelToPlayerSize: false,
      nudgeMaxRetry: 10,
    });

    hls.loadSource(url);
    hls.attachMedia(videoEl);

    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      playerLoading = false;
      videoEl?.play().catch(() => {});
    });

    hls.on(Hls.Events.FRAG_LOADED, () => {
      if (playerError) playerError = '';
      playerLoading = false;
      retryCount = 0;
    });

    hls.on(Hls.Events.LEVEL_SWITCHED, () => { if (playerError) playerError = ''; });

    hls.on(Hls.Events.ERROR, (_event, data) => {
      // Non-fatal: handle buffer stalls silently
      if (!data.fatal) {
        if (data.details === 'bufferStalledError' && hls && videoEl) {
          // Seek forward to unstick
          const buffered = videoEl.buffered;
          if (buffered.length > 0) {
            const bufEnd = buffered.end(buffered.length - 1);
            if (bufEnd > videoEl.currentTime + 1) videoEl.currentTime = bufEnd - 0.5;
          }
          hls.startLoad();
        }
        return;
      }
      if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
        hls?.startLoad();
        scheduleRetry(() => { hls?.startLoad(); });
      } else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
        hlsRecoveryCount++;
        if (hlsRecoveryCount <= 3) { hls?.recoverMediaError(); }
        else { hls?.swapAudioCodec(); hls?.recoverMediaError(); hlsRecoveryCount = 0; }
      } else {
        scheduleRetry(() => { if (playingChannel) setupStream(playingChannel.stream_url); });
      }
    });
  }

  function setupMpegts(url: string) {
    if (!videoEl) return;
    // Clean up previous instance for reconnects
    if (mpegtsPlayer) {
      try { mpegtsPlayer.pause(); mpegtsPlayer.unload(); mpegtsPlayer.detachMediaElement(); mpegtsPlayer.destroy(); } catch {}
      mpegtsPlayer = null;
    }
    mpegtsPlayer = mpegts.createPlayer({
      type: 'mpegts', isLive: true, url,
    }, {
      enableWorker: true,
      enableStashBuffer: true,
      stashInitialSize: 2 * 1024 * 1024,  // 2MB for faster start
      autoCleanupSourceBuffer: true,
      autoCleanupMaxBackwardDuration: 120,
      autoCleanupMinBackwardDuration: 60,
      fixAudioTimestampGap: true,
      lazyLoad: true,
      lazyLoadMaxDuration: 180,            // preload 3 minutes
      lazyLoadRecoverDuration: 60,
      seekType: 'range',
    });
    mpegtsPlayer.attachMediaElement(videoEl);
    mpegtsPlayer.load();
    videoEl.play().catch(() => {});

    mpegtsPlayer.on(mpegts.Events.ERROR, () => {
      // Auto-reconnect without full stream restart
      scheduleRetry(() => { if (currentStreamUrl) setupMpegts(currentStreamUrl); });
    });
    mpegtsPlayer.on(mpegts.Events.STATISTICS_INFO, () => {
      if (playerLoading) playerLoading = false;
      if (playerError) playerError = '';
      retryCount = 0;
    });
  }

  function scheduleRetry(action: () => void) {
    if (retryTimer) clearTimeout(retryTimer);
    retryCount++;
    if (retryCount <= MAX_RETRIES) {
      // Instant first retry, then fast backoff
      const delay = retryCount === 1 ? 100 : retryCount <= 3 ? retryCount * 500 : Math.min(1000 * Math.pow(2, retryCount - 4), 16000);
      playerError = '';
      playerLoading = true;
      retryTimer = setTimeout(action, delay);
    } else {
      playerLoading = false;
      playerError = 'Stream unavailable';
    }
  }

  function destroyPlayer() {
    stopStallWatchdog();
    if (retryTimer) { clearTimeout(retryTimer); retryTimer = null; }
    if (hls) { hls.destroy(); hls = null; }
    if (mpegtsPlayer) {
      try { mpegtsPlayer.pause(); mpegtsPlayer.unload(); mpegtsPlayer.detachMediaElement(); mpegtsPlayer.destroy(); } catch {}
      mpegtsPlayer = null;
    }
    if (videoEl) videoEl.removeAttribute('src');
    currentStreamUrl = '';
  }

  function togglePlay() {
    if (!videoEl) return;
    if (videoEl.paused) {
      videoEl.play().catch(() => {});
    } else {
      videoEl.pause();
    }
  }

  function toggleFullscreen() {
    if (!playerContainerEl) return;
    if (document.fullscreenElement) {
      document.exitFullscreen();
    } else {
      playerContainerEl.requestFullscreen();
    }
  }

  function handleFullscreenChange() {
    isFullscreen = !!document.fullscreenElement;
  }

  function closeFavMenu(e: MouseEvent) {
    if (favMenuChannelId !== null && !(e.target as HTMLElement)?.closest('.fav-wrapper')) {
      favMenuChannelId = null;
    }
  }

  onMount(async () => {
    await loadPlaylists();
    document.addEventListener('fullscreenchange', handleFullscreenChange);
    document.addEventListener('click', closeFavMenu);
    document.addEventListener('keydown', handleGlobalKeydown);
    try { externalPlayers = await detectExternalPlayers(); } catch {}

    // Restore browsing state if returning from another page (in-session navigation)
    const saved = get(browsingState);
    if (saved && $selectedPlaylist) {
      activeTab = saved.activeTab;
      tabCache = saved.tabCache;
      searchQuery = saved.searchQuery;
      searchResults = saved.searchResults;
      restoreTabState(saved.activeTab);
      browsingState.set(null);
      if (searchResults.length) loadFavStatus(searchResults);
      else if (groupChannels.length) loadFavStatus(groupChannels);
    } else if (!$selectedPlaylist) {
      // Fresh app start — restore last playlist and tab from settings
      try {
        const [lastId, lastTab] = await Promise.all([
          getSetting('last_playlist_id'),
          getSetting('last_active_tab'),
        ]);
        if (lastId) {
          const id = parseInt(lastId);
          const found = $playlists.find(p => p.id === id);
          if (found) {
            selectedPlaylist.set(found);
            const tab = (lastTab as 'live' | 'vod' | 'series') || 'live';
            activeTab = ['live', 'vod', 'series'].includes(tab) ? tab : 'live';
            loadContentTypeCounts(found.id);
            loadGroupsForTab(found.id, activeTab);
          }
        }
      } catch {}
    }
  });

  onDestroy(() => {
    // Save current tab into cache, then persist everything
    saveCurrentTabState();
    browsingState.set({
      activeTab,
      searchQuery,
      searchResults,
      tabCache,
    });

    destroyPlayer();
    if (usingMpv) mpvStop().catch(() => {});
    document.removeEventListener('fullscreenchange', handleFullscreenChange);
    document.removeEventListener('click', closeFavMenu);
    document.removeEventListener('keydown', handleGlobalKeydown);
  });

  function selectPlaylist(p: typeof $selectedPlaylist) {
    selectedPlaylist.set(p);
    selectedGroup = null;
    groupChannels = [];
    tabGroups = [];
    tabCache = {};
    stopPlaying();
    if (p) {
      loadContentTypeCounts(p.id);
      activeTab = 'live';
      loadGroupsForTab(p.id, 'live');
      setSetting('last_playlist_id', String(p.id)).catch(() => {});
      setSetting('last_active_tab', 'live').catch(() => {});
    }
  }

  async function loadGroupsForTab(playlistId: number, type: string) {
    tabGroupsLoading = true;
    selectedGroup = null;
    groupChannels = [];
    try {
      tabGroups = await getGroupsByType(playlistId, type);
    } catch (e) {
      console.error('Failed to load groups:', e);
      tabGroups = [];
    } finally {
      tabGroupsLoading = false;
    }
  }

  function switchTab(tab: 'live' | 'vod' | 'series') {
    if (tab === activeTab) return;
    // Save current tab's state before switching
    saveCurrentTabState();
    activeTab = tab;
    setSetting('last_active_tab', tab).catch(() => {});
    // Try to restore cached state for the target tab
    const restored = restoreTabState(tab);
    if (!restored) {
      const p = $selectedPlaylist;
      if (p) loadGroupsForTab(p.id, tab);
    }
    // Re-run search for the new tab if there's an active query
    if (searchQuery.trim()) {
      handleSearchInput(searchQuery);
    }
  }

  const RECENTLY_ADDED = '__recently_added__';

  async function selectGroup(groupName: string) {
    const p = $selectedPlaylist;
    if (!p) return;
    closeSeries();
    selectedGroup = groupName;
    channelOffset = 0;
    groupChannels = [];
    channelsLoading = true;
    try {
      if (groupName === RECENTLY_ADDED) {
        const data = await getRecentlyAdded(p.id, activeTab, PAGE_SIZE);
        groupChannels = data;
        hasMore = false;
      } else {
        const data = await getChannelsByGroup(p.id, activeTab, groupName, PAGE_SIZE, 0);
        groupChannels = data;
        hasMore = data.length === PAGE_SIZE;
      }
      loadFavStatus(groupChannels);
    } catch (e) {
      console.error('Failed to load channels:', e);
      groupChannels = [];
      hasMore = false;
    } finally {
      channelsLoading = false;
    }
  }

  async function loadMore() {
    const p = $selectedPlaylist;
    if (!p || !selectedGroup || channelsLoading) return;
    channelsLoading = true;
    const newOffset = channelOffset + PAGE_SIZE;
    try {
      const data = await getChannelsByGroup(p.id, activeTab, selectedGroup, PAGE_SIZE, newOffset);
      groupChannels = [...groupChannels, ...data];
      channelOffset = newOffset;
      hasMore = data.length === PAGE_SIZE;
    } catch (e) {
      console.error('Failed to load more:', e);
    } finally {
      channelsLoading = false;
    }
  }

  let usingMpv = $state(false);

  async function playChannel(ch: Channel) {
    // Record previous viewing
    if (playingChannel && playStartTime) {
      const duration = Math.floor((Date.now() - playStartTime) / 1000);
      if (duration > 5) {
        recordViewing(playingChannel.id, duration).catch(() => {});
      }
    }

    const type = detectStreamType(ch.stream_url);
    const isLive = type === 'hls' || type === 'ts';

    // For live streams → use MPV (perfect quality, no cuts)
    if (isLive) {
      destroyPlayer();
      playingChannel = ch;
      playStartTime = Date.now();
      playerError = '';
      playerLoading = true;
      usingMpv = true;

      try {
        // If MPV is already running, just switch the URL (instant channel change)
        const running = await mpvIsRunning();
        if (running) {
          await mpvLoad(ch.stream_url, ch.name);
        } else {
          await mpvPlay(ch.stream_url, ch.name);
        }
        playerLoading = false;
      } catch (e) {
        playerError = `MPV: ${e}`;
        playerLoading = false;
        usingMpv = false;
      }
    } else {
      // VOD/series → always use external player (MPV/VLC)
      destroyPlayer();
      playingChannel = ch;
      playStartTime = Date.now();
      playerError = '';
      usingMpv = false;
      await openInExternalPlayer(ch.stream_url);
    }
  }

  async function stopPlaying() {
    if (playingChannel && playStartTime) {
      const duration = Math.floor((Date.now() - playStartTime) / 1000);
      if (duration > 5) {
        try { await recordViewing(playingChannel.id, duration); } catch {}
      }
    }
    if (usingMpv) {
      mpvStop().catch(() => {});
      usingMpv = false;
    }
    destroyPlayer();
    playingChannel = null;
    playStartTime = null;
    playerLoading = false;
    playerPaused = false;
    playerError = '';
    vodPlayingExternal = false;
    if (document.fullscreenElement) document.exitFullscreen();
  }

  let seriesError = $state('');

  // Favorites
  const FAV_CATEGORIES = ['General', 'News', 'Sports', 'Movies', 'Kids', 'Music', 'Documentary', 'Entertainment'];

  // Search
  let searchQuery = $state('');
  let searchResults = $state<Channel[]>([]);
  let searchLoading = $state(false);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let isSearching = $derived(searchQuery.length > 0);
  let searchInputEl = $state<HTMLInputElement | undefined>();

  function handleSearchInput(value: string) {
    searchQuery = value;
    if (searchTimer) clearTimeout(searchTimer);
    if (!value.trim()) {
      searchResults = [];
      searchLoading = false;
      return;
    }
    searchLoading = true;
    // Debounce 200ms for fast feel
    searchTimer = setTimeout(async () => {
      const p = $selectedPlaylist;
      if (!p || !value.trim()) { searchResults = []; searchLoading = false; return; }
      try {
        // Search by name AND group name, ranked by relevance
        searchResults = await searchChannelsInPlaylist(p.id, activeTab, value.trim(), 100);
        loadFavStatus(searchResults);
      } catch {
        searchResults = [];
      } finally {
        searchLoading = false;
      }
    }, 200);
  }

  // Ctrl+K to focus search
  function handleGlobalKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
      e.preventDefault();
      searchInputEl?.focus();
    }
    if (e.key === 'Escape' && isSearching) {
      clearSearch();
    }
  }

  function clearSearch() {
    searchQuery = '';
    searchResults = [];
    searchLoading = false;
    if (searchTimer) { clearTimeout(searchTimer); searchTimer = null; }
  }

  function playFromSearch(ch: Channel) {
    selectedGroup = ch.group_name;
    groupChannels = [ch];
    hasMore = false;
    if (activeTab === 'series') openSeries(ch);
    else if (activeTab === 'vod') openVod(ch);
    else playChannel(ch);
  }

  let downloadingSet = $state<Set<number>>(new Set());

  function getExtFromUrl(url: string): string {
    const last = url.split('/').pop() || '';
    const ext = last.split('.').pop() || '';
    return ['mp4', 'mkv', 'avi', 'flv', 'ts', 'webm', 'mov'].includes(ext) ? ext : 'mp4';
  }

  async function downloadChannel(ch: Channel) {
    if (downloadingSet.has(ch.id)) return;
    const ext = getExtFromUrl(ch.stream_url);
    // VOD: "Movie Name.mp4"
    const filename = `${ch.name}.${ext}`;
    downloadingSet = new Set([...downloadingSet, ch.id]);
    try {
      await startDownload(ch.stream_url, filename, ch.id);
    } catch (e) {
      console.error('Download failed:', e);
    }
  }

  let downloadingEps = $state<Set<string>>(new Set());

  async function downloadEpisode(streamUrl: string, title: string, seasonNum?: string, epNum?: string) {
    const key = `${seasonNum}-${epNum}`;
    if (downloadingEps.has(key)) return;

    const ext = getExtFromUrl(streamUrl);
    let filename: string;
    if (seasonNum && epNum) {
      const s = seasonNum.padStart(2, '0');
      const e = epNum.padStart(2, '0');
      filename = `${title} - S${s}E${e}.${ext}`;
    } else {
      filename = `${title}.${ext}`;
    }

    downloadingEps = new Set([...downloadingEps, key]);
    try {
      await startDownload(streamUrl, filename, 0);
    } catch (e) {
      console.error('Download failed:', e);
      downloadingEps = new Set([...downloadingEps].filter(k => k !== key));
    }
  }
  let favSet = $state<Set<number>>(new Set());
  let favMenuChannelId = $state<number | null>(null);

  async function toggleFav(channelId: number, category: string) {
    favMenuChannelId = null;
    if (favSet.has(channelId)) {
      await removeFavorite(channelId);
      favSet = new Set([...favSet].filter(id => id !== channelId));
    } else {
      await addFavorite(channelId, category);
      favSet = new Set([...favSet, channelId]);
    }
  }

  async function quickToggleFav(channelId: number) {
    if (favSet.has(channelId)) {
      await removeFavorite(channelId);
      favSet = new Set([...favSet].filter(id => id !== channelId));
    } else {
      // Show category picker
      favMenuChannelId = channelId;
    }
  }

  // Load fav status when channels load
  async function loadFavStatus(channels: Channel[]) {
    const newSet = new Set(favSet);
    for (const ch of channels) {
      try {
        if (await isFavorite(ch.id)) newSet.add(ch.id);
      } catch {}
    }
    favSet = newSet;
  }

  async function openSeries(ch: Channel) {
    const p = $selectedPlaylist;
    if (!p) { console.error('No playlist selected'); return; }
    if (p.source_type !== 'xtream') { console.error('Not xtream playlist:', p.source_type); return; }
    if (!p.source_url || !p.xtream_username || !p.xtream_password) { console.error('Missing xtream credentials'); return; }

    viewingSeries = ch;
    seriesDetail = null;
    seriesLoading = true;
    seriesError = '';
    selectedSeason = null;

    // Extract series_id from the stream_url: http://server/series/user/pass/{id}
    const parts = ch.stream_url.split('/');
    const seriesId = parseInt(parts[parts.length - 1]);
    if (isNaN(seriesId)) {
      seriesError = `Could not extract series ID from URL: ${ch.stream_url}`;
      seriesLoading = false;
      return;
    }

    try {
      seriesDetail = await getSeriesInfo(p.source_url, p.xtream_username, p.xtream_password, seriesId);
      if (seriesDetail.seasons.length > 0) {
        selectedSeason = seriesDetail.seasons[0].season_number;
      }
    } catch (e) {
      seriesError = String(e);
      console.error('Failed to load series info:', e);
    } finally {
      seriesLoading = false;
    }
  }

  function closeSeries() {
    viewingSeries = null;
    seriesDetail = null;
    selectedSeason = null;
  }

  async function openVod(ch: Channel) {
    const p = $selectedPlaylist;
    if (!p) return;
    if (p.source_type !== 'xtream') {
      // Non-xtream: just play directly, no detail page
      await openInExternalPlayer(ch.stream_url);
      return;
    }
    if (!p.source_url || !p.xtream_username || !p.xtream_password) return;

    viewingVod = ch;
    vodDetail = null;
    vodLoading = true;
    vodError = '';

    // Extract vod_id from stream_url: http://server/movie/user/pass/{id}.ext
    const filename = ch.stream_url.split('/').pop() || '';
    const vodId = parseInt(filename.split('.')[0]);
    if (isNaN(vodId)) {
      vodError = `Could not extract VOD ID from URL: ${ch.stream_url}`;
      vodLoading = false;
      return;
    }

    try {
      vodDetail = await getVodInfo(p.source_url, p.xtream_username, p.xtream_password, vodId);
    } catch (e) {
      vodError = String(e);
    } finally {
      vodLoading = false;
    }
  }

  function closeVod() {
    viewingVod = null;
    vodDetail = null;
    vodError = '';
  }

  async function playVod() {
    if (!viewingVod) return;
    await openInExternalPlayer(viewingVod.stream_url);
  }

  async function playEpisode(streamUrl: string, title: string) {
    if (playingChannel && playStartTime) {
      const duration = Math.floor((Date.now() - playStartTime) / 1000);
      if (duration > 5) recordViewing(playingChannel.id, duration).catch(() => {});
    }
    destroyPlayer();
    playingChannel = {
      id: 0, playlist_id: 0, name: title, group_name: '',
      stream_url: streamUrl, logo_url: viewingSeries?.logo_url ?? null,
      epg_id: null, content_type: 'series', created_at: '',
    };
    playStartTime = Date.now();
    playerError = '';
    usingMpv = false;
    await openInExternalPlayer(streamUrl);
  }

  function getBackdrop(info: { backdrop_path?: unknown }): string | null {
    if (!info.backdrop_path) return null;
    if (Array.isArray(info.backdrop_path) && info.backdrop_path.length > 0) return info.backdrop_path[0];
    if (typeof info.backdrop_path === 'string') return info.backdrop_path;
    return null;
  }

  function getCountForType(type_: string): number {
    return $contentTypeCounts.find(c => c.content_type === type_)?.count ?? 0;
  }

  async function handleDelete() {
    const p = $selectedPlaylist;
    if (!p) return;
    actionLoading = true;
    try {
      await deletePlaylist(p.id);
      selectedPlaylist.set(null);
      tabGroups = [];
      groupChannels = [];
      stopPlaying();
      await loadPlaylists();
    } catch (e) {
      console.error('Failed to delete:', e);
    } finally {
      actionLoading = false;
    }
  }

  async function handleExport() {
    const p = $selectedPlaylist;
    if (!p) return;
    const outputPath = `${p.name.replace(/[^a-zA-Z0-9]/g, '_')}.m3u`;
    actionLoading = true;
    try {
      await exportPlaylist(p.id, outputPath);
    } catch (e) {
      console.error('Failed to export:', e);
    } finally {
      actionLoading = false;
    }
  }

  let refreshing = $state(false);
  let refreshProgress = $state('');

  async function handleRefresh() {
    const p = $selectedPlaylist;
    if (!p || refreshing) return;
    // Only xtream and m3u_url can be refreshed
    if (p.source_type !== 'xtream' && p.source_type !== 'm3u_url') return;

    refreshing = true;
    refreshProgress = 'Starting...';

    // Listen for progress events
    let unlisten: (() => void) | null = null;
    try {
      const { listen } = await import('@tauri-apps/api/event');
      unlisten = await listen<{ stage: string; current: number; total: number }>('xtream-import-progress', (event) => {
        refreshProgress = event.payload.stage;
      });
    } catch {}

    try {
      const updated = await refreshPlaylist(p.id);
      selectedPlaylist.set(updated);
      await loadPlaylists();
      loadContentTypeCounts(p.id);
      // Reload groups for current tab
      loadGroupsForTab(p.id, activeTab);
    } catch (e) {
      console.error('Failed to refresh:', e);
    } finally {
      refreshing = false;
      refreshProgress = '';
      unlisten?.();
    }
  }
</script>

<div class="playlists-page fade-in">
  <!-- Column 1: Playlists -->
  <div class="col col-playlists">
    <div class="col-header">
      <h2>Playlists</h2>
      <button class="btn-accent add-btn" onclick={() => showAddModal = true} title="Add playlist">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
      </button>
    </div>
    <div class="col-scroll">
      {#each $playlists as p (p.id)}
        <PlaylistItem
          playlist={p}
          active={$selectedPlaylist?.id === p.id}
          onclick={() => selectPlaylist(p)}
        />
      {/each}
      {#if $playlists.length === 0}
        <div class="col-empty">No playlists</div>
      {/if}
    </div>
  </div>

  {#if !$selectedPlaylist}
    <!-- Welcome page spanning columns 2+3 -->
    <div class="welcome-page">
      <div class="welcome-content">
        <div class="welcome-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2">
            <rect x="2" y="3" width="20" height="14" rx="3"/>
            <path d="M8 21h8M12 17v4"/>
            <polygon points="10 7.5 10 12.5 14.5 10 10 7.5" fill="currentColor" stroke="none"/>
          </svg>
        </div>
        <h2 class="welcome-title">Welcome to JoTV</h2>
        <p class="welcome-desc">Select a playlist from the left to start watching, or add a new one.</p>

        <div class="welcome-features">
          <div class="welcome-feature">
            <div class="feature-icon live">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg>
            </div>
            <div class="feature-text">
              <strong>Live TV</strong>
              <span>Watch live channels with MPV</span>
            </div>
          </div>
          <div class="welcome-feature">
            <div class="feature-icon vod">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            </div>
            <div class="feature-text">
              <strong>Movies</strong>
              <span>Browse and play VOD content</span>
            </div>
          </div>
          <div class="welcome-feature">
            <div class="feature-icon series">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="20" height="20" rx="2"/><path d="M7 2v20M17 2v20M2 12h20"/></svg>
            </div>
            <div class="feature-text">
              <strong>Series</strong>
              <span>Seasons, episodes & details</span>
            </div>
          </div>
        </div>

        {#if $playlists.length === 0}
          <button class="welcome-add-btn" onclick={() => showAddModal = true}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            Add Your First Playlist
          </button>
        {:else}
          <p class="welcome-hint">Select a playlist to get started</p>
        {/if}
      </div>
    </div>
  {:else}
  <!-- Column 2: Groups -->
  <div class="col col-groups">
      <div class="col-header">
        <h2 class="truncate" title={$selectedPlaylist.name}>{$selectedPlaylist.name}</h2>
        <div class="header-actions">
          {#if $selectedPlaylist.source_type === 'xtream' || $selectedPlaylist.source_type === 'm3u_url'}
            <button class="icon-btn" class:refreshing={refreshing} onclick={handleRefresh} disabled={refreshing || actionLoading} title="Refresh channels">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/></svg>
            </button>
          {/if}
          <button class="icon-btn" onclick={handleExport} disabled={actionLoading} title="Export">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
          </button>
          <button class="icon-btn danger" onclick={handleDelete} disabled={actionLoading} title="Delete">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
          </button>
        </div>
      </div>

      <!-- Refresh progress bar -->
      {#if refreshing}
        <div class="refresh-bar">
          <div class="refresh-spinner"></div>
          <span class="refresh-text">{refreshProgress}</span>
        </div>
      {/if}

      <div class="tabs">
        <button class="tab" class:active={activeTab === 'live'} onclick={() => switchTab('live')}>
          Live <span class="tab-count">{getCountForType('live')}</span>
        </button>
        <button class="tab" class:active={activeTab === 'vod'} onclick={() => switchTab('vod')}>
          VOD <span class="tab-count">{getCountForType('vod')}</span>
        </button>
        <button class="tab" class:active={activeTab === 'series'} onclick={() => switchTab('series')}>
          Series <span class="tab-count">{getCountForType('series')}</span>
        </button>
      </div>

      <!-- Search input -->
      <div class="search-box">
        <svg class="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
        <input
          bind:this={searchInputEl}
          type="text"
          class="search-input"
          placeholder="Search {activeTab} by name or group..."
          value={searchQuery}
          oninput={(e) => handleSearchInput((e.target as HTMLInputElement).value)}
        />
        {#if searchQuery}
          <button class="search-clear" onclick={clearSearch} title="Clear (Esc)">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        {:else}
          <kbd class="search-kbd">Ctrl+K</kbd>
        {/if}
      </div>

      <div class="col-scroll">
        {#if isSearching}
          <!-- Search results shown in groups column -->
          {#if searchLoading}
            <div class="col-empty">Searching...</div>
          {:else if searchResults.length === 0}
            <div class="col-empty">No results for "{searchQuery}"</div>
          {:else}
            <div class="search-results-count">{searchResults.length} results</div>
            {#each searchResults as ch (ch.id)}
              <div class="search-row">
                <button class="group-item" onclick={() => playFromSearch(ch)}>
                  {#if ch.logo_url}
                    <img class="search-thumb" src={ch.logo_url} alt="" loading="lazy" />
                  {:else}
                    <div class="group-icon">
                      {#if activeTab === 'series'}
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="20" height="20" rx="2"/><path d="M7 2v20M17 2v20M2 12h20"/></svg>
                      {:else}
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg>
                      {/if}
                    </div>
                  {/if}
                  <div class="search-result-text">
                    <span class="group-name">{ch.name}</span>
                    <span class="search-result-group">{ch.group_name}</span>
                  </div>
                </button>
                <div class="fav-wrapper">
                  <button
                    class="fav-btn"
                    class:is-fav={favSet.has(ch.id)}
                    onclick={(e) => { e.stopPropagation(); quickToggleFav(ch.id); }}
                    title={favSet.has(ch.id) ? 'Remove from favorites' : 'Add to favorites'}
                  >
                    {#if favSet.has(ch.id)}
                      <svg viewBox="0 0 24 24" fill="currentColor" stroke="none"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
                    {:else}
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
                    {/if}
                  </button>
                  {#if favMenuChannelId === ch.id}
                    <div class="fav-menu">
                      {#each FAV_CATEGORIES as cat}
                        <button class="fav-menu-item" onclick={(e) => { e.stopPropagation(); toggleFav(ch.id, cat); }}>{cat}</button>
                      {/each}
                    </div>
                  {/if}
                </div>
              </div>
            {/each}
          {/if}
        {:else if tabGroupsLoading}
          <div class="col-empty">Loading...</div>
        {:else if tabGroups.length === 0}
          <div class="col-empty">No {activeTab} content</div>
        {:else}
          <!-- Recently Added (VOD & Series only) -->
          {#if activeTab === 'vod' || activeTab === 'series'}
            <button
              class="group-item recently-added"
              class:active={selectedGroup === RECENTLY_ADDED}
              onclick={() => selectGroup(RECENTLY_ADDED)}
            >
              <div class="group-icon new">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
              </div>
              <span class="group-name">Recently Added</span>
              <svg class="new-badge" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/></svg>
            </button>
          {/if}
          {#each tabGroups as group (group.name)}
            <button
              class="group-item"
              class:active={selectedGroup === group.name}
              onclick={() => selectGroup(group.name)}
            >
              <div class="group-icon">
                {#if activeTab === 'live'}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg>
                {:else if activeTab === 'vod'}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                {:else}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="20" height="20" rx="2"/><path d="M7 2v20M17 2v20M2 12h20M2 7h5M2 17h5M17 7h5M17 17h5"/></svg>
                {/if}
              </div>
              <span class="group-name">{group.name || 'Uncategorized'}</span>
              <span class="group-count">{group.count}</span>
            </button>
          {/each}
        {/if}
      </div>
  </div>

  <!-- Column 3: Channels + Inline Player -->
  <div class="col col-channels">
    {#if selectedGroup !== null}
      <!-- Player panel (live only — VOD/series open in external player) -->
      {#if playingChannel && usingMpv}
        <div class="mpv-panel">
          <div class="mpv-display">
            {#if playingChannel.logo_url}
              <img class="mpv-logo" src={playingChannel.logo_url} alt="" />
            {:else}
              <div class="mpv-logo placeholder">
                <span>{playingChannel.name.charAt(0).toUpperCase()}</span>
              </div>
            {/if}
            <div class="mpv-info">
              <h3>{playingChannel.name}</h3>
              <span class="mpv-group">{playingChannel.group_name}</span>
              {#if playerLoading}
                <span class="mpv-status loading">Starting MPV...</span>
              {:else if playerError}
                <span class="mpv-status error">{playerError}</span>
              {:else}
                <span class="mpv-status live">Playing in MPV</span>
              {/if}
            </div>
          </div>
          <div class="mpv-controls">
            <button class="mpv-btn" onclick={() => mpvPause()} title="Pause/Play">
              <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
            </button>
            <button class="mpv-btn" onclick={() => mpvFullscreen()} title="Fullscreen">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><polyline points="21 3 14 10"/><polyline points="3 21 10 14"/></svg>
            </button>
            <button class="mpv-btn stop" onclick={stopPlaying} title="Stop">
              <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="1"/></svg>
            </button>
          </div>
        </div>
      {/if}

      <!-- Series detail page -->
      {#if viewingSeries && activeTab === 'series'}
        <div class="vod-detail">
          {#if seriesLoading}
            <div class="col-empty">Loading series info...</div>
          {:else if seriesDetail}
            {@const info = seriesDetail.info}
            {@const backdrop = getBackdrop(info) || info.cover || viewingSeries?.logo_url}
            <div class="vod-hero" style={backdrop ? `background-image: url(${backdrop})` : ''}>
              <div class="vod-hero-overlay">
                {#if info.cover || viewingSeries.logo_url}
                  <img class="vod-poster" src={info.cover || viewingSeries.logo_url} alt="" />
                {/if}
                <div class="vod-meta">
                  <h2>{info.name || viewingSeries.name}</h2>
                  {#if info.genre}
                    <span class="vod-genre">{info.genre}</span>
                  {/if}
                  <div class="vod-tags">
                    {#if info.rating}
                      <span class="vod-tag">&#9733; {info.rating}</span>
                    {/if}
                    {#if info.release_date}
                      <span class="vod-tag">{info.release_date}</span>
                    {/if}
                    <span class="vod-tag">{seriesDetail.seasons.length} seasons</span>
                  </div>
                  {#if info.director}
                    <p class="vod-director"><strong>Director:</strong> {info.director}</p>
                  {/if}
                  {#if info.cast}
                    <p class="vod-cast"><strong>Cast:</strong> {info.cast}</p>
                  {/if}
                  {#if info.plot}
                    <p class="vod-plot">{info.plot}</p>
                  {/if}
                </div>
                <button class="vod-close-btn" onclick={closeSeries} title="Back">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                </button>
              </div>
            </div>

            <!-- Season tabs + Episodes -->
            <div class="season-tabs">
              {#each seriesDetail.seasons as season (season.season_number)}
                <button
                  class="season-tab"
                  class:active={selectedSeason === season.season_number}
                  onclick={() => selectedSeason = season.season_number}
                >
                  S{season.season_number}
                </button>
              {/each}
            </div>

            <div class="episodes-section">
              {#each seriesDetail.seasons.filter(s => s.season_number === selectedSeason) as season (season.season_number)}
                <div class="episodes-list">
                  {#each season.episodes as ep (ep.id)}
                    <div class="ep-row" class:active={playingChannel?.name === ep.title}>
                      <button class="ep-row-btn" onclick={() => playEpisode(ep.stream_url, ep.title)}>
                        <span class="ep-num">E{ep.episode_num}</span>
                        <span class="ep-title">{ep.title}</span>
                        {#if playingChannel?.name === ep.title}
                          <div class="now-playing">
                            <span class="bar"></span><span class="bar"></span><span class="bar"></span>
                          </div>
                        {/if}
                      </button>
                      <button
                        class="ep-dl-btn"
                        class:downloading={downloadingEps.has(`${season.season_number}-${ep.episode_num}`)}
                        onclick={(e) => { e.stopPropagation(); downloadEpisode(ep.stream_url, viewingSeries?.name || ep.title, season.season_number, ep.episode_num); }}
                        title={downloadingEps.has(`${season.season_number}-${ep.episode_num}`) ? 'Downloading...' : 'Download'}
                      >
                        {#if downloadingEps.has(`${season.season_number}-${ep.episode_num}`)}
                          <svg class="dl-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 11-6.219-8.56"/></svg>
                        {:else}
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                        {/if}
                      </button>
                    </div>
                  {/each}
                </div>
              {/each}
            </div>

            <!-- More from same group -->
            {@const related = groupChannels.filter(ch => ch.id !== viewingSeries?.id)}
            {#if related.length > 0}
              <div class="vod-related">
                <h3 class="vod-related-title">More from {viewingSeries?.group_name || 'this category'}</h3>
                <div class="vod-related-grid">
                  {#each related as ch (ch.id)}
                    <button class="content-card" onclick={() => openSeries(ch)}>
                      <div class="content-card-btn">
                        {#if ch.logo_url}
                          <img class="content-thumb" src={ch.logo_url} alt="" loading="lazy" />
                        {:else}
                          <div class="content-thumb placeholder">
                            <span>{ch.name.charAt(0).toUpperCase()}</span>
                          </div>
                        {/if}
                      </div>
                      <div class="content-card-footer">
                        <span class="content-title" title={ch.name}>{ch.name}</span>
                      </div>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          {:else}
            <div class="vod-error-panel">
              <p>Failed to load series</p>
              {#if seriesError}
                <span class="error-detail">{seriesError}</span>
              {/if}
              <div class="error-actions">
                <button class="btn-ghost" onclick={() => { if (viewingSeries) openSeries(viewingSeries); }}>Retry</button>
                <button class="btn-ghost" onclick={closeSeries}>Back</button>
              </div>
            </div>
          {/if}
        </div>

      <!-- VOD detail overlay -->
      {:else if viewingVod && activeTab === 'vod'}
        <div class="vod-detail">
          {#if vodLoading}
            <div class="col-empty">Loading movie info...</div>
          {:else if vodDetail}
            {@const info = vodDetail.info}
            {@const backdrop = getBackdrop(info) || info.cover || viewingVod?.logo_url}
            <div class="vod-hero" style={backdrop ? `background-image: url(${backdrop})` : ''}>
              <div class="vod-hero-overlay">
                {#if info.cover || viewingVod.logo_url}
                  <img class="vod-poster" src={info.cover || viewingVod.logo_url} alt="" />
                {/if}
                <div class="vod-meta">
                  <h2>{info.name || viewingVod.name}</h2>
                  {#if info.genre}
                    <span class="vod-genre">{info.genre}</span>
                  {/if}
                  <div class="vod-tags">
                    {#if info.rating}
                      <span class="vod-tag">&#9733; {info.rating}</span>
                    {/if}
                    {#if info.release_date}
                      <span class="vod-tag">{info.release_date}</span>
                    {/if}
                    {#if info.duration}
                      <span class="vod-tag">{info.duration}</span>
                    {/if}
                  </div>
                  {#if info.director}
                    <p class="vod-director"><strong>Director:</strong> {info.director}</p>
                  {/if}
                  {#if info.cast}
                    <p class="vod-cast"><strong>Cast:</strong> {info.cast}</p>
                  {/if}
                  {#if info.plot}
                    <p class="vod-plot">{info.plot}</p>
                  {/if}
                  <div class="vod-actions">
                    <button class="vod-play-btn" onclick={playVod}>
                      <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                      Play
                    </button>
                    <button
                      class="vod-dl-btn"
                      class:downloading={dlProgressMap.has(viewingVod.id)}
                      onclick={() => { if (viewingVod) downloadChannel(viewingVod); }}
                    >
                      {#if dlProgressMap.has(viewingVod.id)}
                        <svg class="dl-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 11-6.219-8.56"/></svg>
                        {Math.round((dlProgressMap.get(viewingVod.id)?.progress ?? 0) * 100)}%
                      {:else}
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                        Download
                      {/if}
                    </button>
                  </div>
                </div>
                <button class="vod-close-btn" onclick={closeVod} title="Back">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                </button>
              </div>
            </div>

            <!-- More from same group -->
            {@const related = groupChannels.filter(ch => ch.id !== viewingVod?.id)}
            {#if related.length > 0}
              <div class="vod-related">
                <h3 class="vod-related-title">More from {viewingVod?.group_name || 'this category'}</h3>
                <div class="vod-related-grid">
                  {#each related as ch (ch.id)}
                    <button class="content-card" onclick={() => openVod(ch)}>
                      <div class="content-card-btn">
                        {#if ch.logo_url}
                          <img class="content-thumb" src={ch.logo_url} alt="" loading="lazy" />
                        {:else}
                          <div class="content-thumb placeholder">
                            <span>{ch.name.charAt(0).toUpperCase()}</span>
                          </div>
                        {/if}
                      </div>
                      <div class="content-card-footer">
                        <span class="content-title" title={ch.name}>{ch.name}</span>
                      </div>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          {:else}
            <div class="vod-error-panel">
              <p>Failed to load movie info</p>
              {#if vodError}
                <span class="error-detail">{vodError}</span>
              {/if}
              <div class="error-actions">
                <button class="btn-ghost" onclick={() => { if (viewingVod) openVod(viewingVod); }}>Retry</button>
                <button class="vod-play-btn" onclick={playVod}>
                  <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                  Play Anyway
                </button>
                <button class="btn-ghost" onclick={closeVod}>Back</button>
              </div>
            </div>
          {/if}
        </div>

      <!-- Regular channel list / grid -->
      {:else}
        <div class="col-header">
          <h2 class="truncate" title={selectedGroup === RECENTLY_ADDED ? 'Recently Added' : (selectedGroup || 'Uncategorized')}>{selectedGroup === RECENTLY_ADDED ? 'Recently Added' : (selectedGroup || 'Uncategorized')}</h2>
          <span class="channel-total">{tabGroups.find(g => g.name === selectedGroup)?.count ?? 0}</span>
        </div>

        <div class="col-scroll">
          {#if channelsLoading && groupChannels.length === 0}
            <div class="col-empty">Loading...</div>
          {:else if groupChannels.length === 0}
            <div class="col-empty">No channels</div>
          {:else}
            <!-- Grid cards for all tabs -->
            <div class="content-grid" class:live-grid={activeTab === 'live'}>
              {#each groupChannels as ch (ch.id)}
                <div class="content-card" class:active={playingChannel?.id === ch.id}>
                  <button class="content-card-btn" onclick={() => activeTab === 'series' ? openSeries(ch) : activeTab === 'vod' ? openVod(ch) : playChannel(ch)}>
                    {#if ch.logo_url}
                      <img class="content-thumb" class:landscape={activeTab === 'live'} src={ch.logo_url} alt="" loading="lazy" />
                    {:else}
                      <div class="content-thumb placeholder" class:landscape={activeTab === 'live'}>
                        <span>{ch.name.charAt(0).toUpperCase()}</span>
                      </div>
                    {/if}
                    {#if playingChannel?.id === ch.id}
                      <div class="card-now-playing">
                        <span class="bar"></span><span class="bar"></span><span class="bar"></span>
                      </div>
                    {/if}
                    {#if dlProgressMap.has(ch.id)}
                      {@const dlp = dlProgressMap.get(ch.id)}
                      <div class="card-dl-overlay">
                        <div class="card-dl-bar" style="width: {(dlp?.progress ?? 0) * 100}%"></div>
                        <span class="card-dl-pct">{Math.round((dlp?.progress ?? 0) * 100)}%</span>
                      </div>
                    {/if}
                  </button>
                  <div class="content-card-footer">
                    <span class="content-title" title={ch.name}>{ch.name}</span>
                    <div class="content-card-actions">
                      {#if activeTab === 'vod'}
                        <button
                          class="card-action-btn"
                          class:downloading={downloadingSet.has(ch.id)}
                          onclick={(e) => { e.stopPropagation(); downloadChannel(ch); }}
                          title={downloadingSet.has(ch.id) ? 'Downloading...' : 'Download'}
                        >
                          {#if downloadingSet.has(ch.id)}
                            <svg class="dl-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 11-6.219-8.56"/></svg>
                          {:else}
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                          {/if}
                        </button>
                      {/if}
                      <div class="fav-wrapper">
                        <button
                          class="card-action-btn"
                          class:is-fav={favSet.has(ch.id)}
                          onclick={(e) => { e.stopPropagation(); quickToggleFav(ch.id); }}
                          title={favSet.has(ch.id) ? 'Remove from favorites' : 'Add to favorites'}
                        >
                          {#if favSet.has(ch.id)}
                            <svg viewBox="0 0 24 24" fill="currentColor" stroke="none"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
                          {:else}
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
                          {/if}
                        </button>
                        {#if favMenuChannelId === ch.id}
                          <div class="fav-menu">
                            {#each FAV_CATEGORIES as cat}
                              <button class="fav-menu-item" onclick={(e) => { e.stopPropagation(); toggleFav(ch.id, cat); }}>
                                {cat}
                              </button>
                            {/each}
                          </div>
                        {/if}
                      </div>
                    </div>
                  </div>
                </div>
              {/each}
            </div>
            {#if hasMore}
              <button class="load-more-btn" onclick={loadMore} disabled={channelsLoading}>
                {channelsLoading ? 'Loading...' : 'Load More'}
              </button>
            {/if}
          {/if}
        </div>
      {/if}
    {:else}
      <div class="col-placeholder">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg>
        <p>Select a group</p>
      </div>
    {/if}
  </div>
  {/if}
</div>

{#if showAddModal}
  <AddPlaylistModal onclose={() => { showAddModal = false; loadPlaylists(); }} />
{/if}

<style>
  .playlists-page {
    display: grid;
    grid-template-columns: 240px 300px 1fr;
    height: calc(100vh - 48px);
    margin: -24px;
    background: var(--color-base);
  }

  /* Welcome page */
  .welcome-page {
    grid-column: 2 / -1;
    display: flex;
    align-items: center;
    justify-content: center;
    border-left: 1px solid var(--color-border);
    background: linear-gradient(135deg, var(--color-base) 0%, rgba(233, 69, 96, 0.03) 100%);
  }

  .welcome-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    max-width: 400px;
    text-align: center;
    padding: 40px;
  }

  .welcome-icon {
    width: 72px;
    height: 72px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(233, 69, 96, 0.1);
    border-radius: 20px;
    color: var(--color-accent);
    margin-bottom: 4px;
  }

  .welcome-icon :global(svg) {
    width: 36px;
    height: 36px;
  }

  .welcome-title {
    font-size: 24px;
    font-weight: 700;
    color: var(--color-text);
  }

  .welcome-desc {
    font-size: 14px;
    color: var(--color-text-muted);
    line-height: 1.5;
  }

  .welcome-features {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    margin-top: 8px;
  }

  .welcome-feature {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    text-align: left;
  }

  .feature-icon {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 10px;
    flex-shrink: 0;
  }

  .feature-icon :global(svg) {
    width: 20px;
    height: 20px;
  }

  .feature-icon.live {
    background: rgba(233, 69, 96, 0.1);
    color: var(--color-accent);
  }

  .feature-icon.vod {
    background: rgba(166, 227, 161, 0.1);
    color: var(--color-accent-green);
  }

  .feature-icon.series {
    background: rgba(137, 180, 250, 0.1);
    color: #89b4fa;
  }

  .feature-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .feature-text strong {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
  }

  .feature-text span {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .welcome-add-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 28px;
    border-radius: 10px;
    background: var(--color-accent);
    color: #fff;
    font-size: 14px;
    font-weight: 600;
    margin-top: 8px;
    transition: all 0.15s;
  }

  .welcome-add-btn:hover {
    transform: scale(1.03);
    box-shadow: 0 6px 20px rgba(233, 69, 96, 0.3);
  }

  .welcome-add-btn :global(svg) {
    width: 18px;
    height: 18px;
  }

  .welcome-hint {
    font-size: 12px;
    color: var(--color-text-muted);
    opacity: 0.6;
    margin-top: 4px;
  }

  .truncate {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Columns */
  .col {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-right: 1px solid var(--color-border);
  }

  .col:last-child {
    border-right: none;
  }

  .col-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 14px 12px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .col-header h2 {
    font-size: 15px;
    font-weight: 700;
  }

  .col-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
  }

  .col-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 80px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  .col-placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--color-text-muted);
  }

  .col-placeholder :global(svg) {
    width: 40px;
    height: 40px;
    opacity: 0.2;
  }

  .col-placeholder p {
    font-size: 13px;
  }

  /* Column 1 - Playlists */
  .add-btn {
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 8px;
    flex-shrink: 0;
  }

  .add-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  /* Column 2 - Groups */
  .header-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .icon-btn {
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text-muted);
  }

  .icon-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .icon-btn.danger:hover {
    background: rgba(233, 69, 96, 0.1);
    color: var(--color-accent);
  }

  .icon-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  .icon-btn.refreshing :global(svg) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .refresh-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .refresh-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.1);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  .refresh-text {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  /* Search */
  .search-box {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .search-icon {
    width: 14px;
    height: 14px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--color-text);
    font-size: 13px;
    padding: 4px 0;
    outline: none;
  }

  .search-input::placeholder {
    color: var(--color-text-muted);
    opacity: 0.6;
  }

  .search-clear {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .search-clear:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .search-clear :global(svg) {
    width: 12px;
    height: 12px;
  }

  .search-kbd {
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--color-surface);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border);
    font-family: monospace;
    flex-shrink: 0;
  }

  .search-results-count {
    font-size: 11px;
    color: var(--color-text-muted);
    padding: 6px 10px 4px;
  }

  .search-thumb {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    object-fit: contain;
    background: var(--color-surface);
    flex-shrink: 0;
  }

  .search-result-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .search-result-group {
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .search-row {
    display: flex;
    align-items: center;
    border-radius: 8px;
    transition: background var(--transition-fast);
  }

  .search-row:hover {
    background: var(--color-hover);
  }

  .search-row .group-item {
    flex: 1;
    min-width: 0;
  }

  .search-row:hover .fav-btn {
    opacity: 1;
  }

  .tab {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 9px 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-muted);
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    transition: color var(--transition-fast), border-color var(--transition-fast);
  }

  .tab:hover {
    color: var(--color-text);
  }

  .tab.active {
    color: var(--color-accent);
    border-bottom-color: var(--color-accent);
  }

  .tab-count {
    font-size: 10px;
    background: var(--color-surface);
    padding: 1px 5px;
    border-radius: 6px;
    color: var(--color-text-muted);
  }

  .tab.active .tab-count {
    background: rgba(233, 69, 96, 0.15);
    color: var(--color-accent);
  }

  .group-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text);
    font-size: 13px;
    transition: background var(--transition-fast);
    margin-bottom: 2px;
  }

  .group-item:hover {
    background: var(--color-hover);
  }

  .group-item.active {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }

  .group-icon {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-surface);
    border-radius: 8px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .group-item.active .group-icon {
    background: rgba(233, 69, 96, 0.12);
    color: var(--color-accent);
  }

  .group-icon :global(svg) {
    width: 16px;
    height: 16px;
  }

  .group-item.recently-added {
    margin-bottom: 6px;
    border-bottom: 1px solid var(--color-border);
    padding-bottom: 10px;
    border-radius: 8px 8px 0 0;
  }

  .group-icon.new {
    background: rgba(240, 165, 0, 0.12);
    color: var(--color-accent-yellow);
  }

  .group-item.recently-added.active .group-icon.new {
    background: rgba(233, 69, 96, 0.12);
    color: var(--color-accent);
  }

  .new-badge {
    width: 14px;
    height: 14px;
    color: var(--color-accent-yellow);
    flex-shrink: 0;
  }

  .group-name {
    flex: 1;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .group-count {
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-surface);
    padding: 1px 7px;
    border-radius: 8px;
    flex-shrink: 0;
  }

  /* MPV player panel */
  .mpv-panel {
    flex-shrink: 0;
    border-bottom: 1px solid var(--color-border);
    padding: 20px;
    background: linear-gradient(135deg, var(--color-card), var(--color-surface));
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .mpv-display {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .mpv-logo {
    width: 64px;
    height: 64px;
    border-radius: 12px;
    object-fit: contain;
    background: var(--color-surface);
    flex-shrink: 0;
  }

  .mpv-logo.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .mpv-logo.placeholder span {
    font-size: 24px;
    font-weight: 700;
    color: var(--color-accent);
    opacity: 0.5;
  }

  .mpv-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .mpv-info h3 {
    font-size: 16px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mpv-group {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .mpv-status {
    font-size: 11px;
    font-weight: 600;
  }

  .mpv-status.live {
    color: var(--color-accent-green);
  }

  .mpv-status.loading {
    color: var(--color-accent-yellow);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  .mpv-status.error {
    color: var(--color-accent);
  }

  .mpv-controls {
    display: flex;
    gap: 8px;
  }

  .mpv-btn {
    height: 36px;
    padding: 0 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text);
    font-size: 12px;
    font-weight: 500;
    transition: all var(--transition-fast);
  }

  .mpv-btn:hover {
    background: var(--color-hover);
  }

  .mpv-btn.stop {
    color: var(--color-accent);
  }

  .mpv-btn.stop:hover {
    background: rgba(233, 69, 96, 0.1);
  }

  .mpv-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  /* Column 3 - Inline Player + Channels */
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .channel-total {
    font-size: 12px;
    color: var(--color-text-muted);
    background: var(--color-surface);
    padding: 2px 8px;
    border-radius: 8px;
    flex-shrink: 0;
  }

  .dl-spin { animation: spin 1s linear infinite; }

  .ep-dl-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    opacity: 0;
    transition: all var(--transition-fast);
    flex-shrink: 0;
    margin-right: 6px;
  }

  .ep-row:hover .ep-dl-btn { opacity: 1; }
  .ep-dl-btn.downloading { opacity: 1; color: var(--color-accent-green); }
  .ep-dl-btn:hover { background: var(--color-surface); color: var(--color-accent-green); }
  .ep-dl-btn :global(svg) { width: 15px; height: 15px; }

  .fav-wrapper {
    position: relative;
    flex-shrink: 0;
    padding-right: 6px;
  }

  .fav-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    opacity: 0;
    transition: all var(--transition-fast);
  }

  .fav-btn.is-fav {
    opacity: 1;
  }

  .fav-btn.is-fav {
    color: var(--color-accent);
  }

  .fav-btn:hover {
    background: var(--color-surface);
  }

  .fav-btn :global(svg) {
    width: 15px;
    height: 15px;
  }

  .fav-menu {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 50;
    background: var(--color-sidebar);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 4px;
    min-width: 140px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
  }

  .fav-menu-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 7px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text);
    background: transparent;
  }

  .fav-menu-item:hover {
    background: var(--color-hover);
    color: var(--color-accent);
  }


  /* Now playing bars animation */
  .now-playing {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 14px;
    flex-shrink: 0;
  }

  .now-playing .bar {
    width: 3px;
    background: var(--color-accent);
    border-radius: 1px;
    animation: bars 0.8s ease-in-out infinite alternate;
  }

  .now-playing .bar:nth-child(1) { height: 40%; animation-delay: 0s; }
  .now-playing .bar:nth-child(2) { height: 70%; animation-delay: 0.2s; }
  .now-playing .bar:nth-child(3) { height: 50%; animation-delay: 0.4s; }

  @keyframes bars {
    0% { height: 30%; }
    100% { height: 100%; }
  }

  .load-more-btn {
    display: block;
    width: 100%;
    padding: 10px;
    margin-top: 4px;
    font-size: 12px;
    color: var(--color-text-muted);
    background: var(--color-surface);
    border-radius: 8px;
    text-align: center;
  }

  .load-more-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  /* Series thumbnail grid */
  .content-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 12px;
    padding: 8px;
  }

  .content-grid.live-grid {
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  }

  .content-card {
    display: flex;
    flex-direction: column;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    overflow: hidden;
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .content-card:hover {
    transform: translateY(-3px);
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.35);
  }

  .content-card.active {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 1px var(--color-accent);
  }

  .content-card-btn {
    position: relative;
    display: block;
    width: 100%;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
  }

  .content-thumb {
    width: 100%;
    aspect-ratio: 2 / 3;
    object-fit: cover;
    background: var(--color-surface);
    display: block;
  }

  .content-thumb.landscape {
    aspect-ratio: 16 / 10;
    object-fit: contain;
  }

  .content-thumb.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .content-thumb.placeholder span {
    font-size: 32px;
    font-weight: 700;
    color: var(--color-accent);
    opacity: 0.3;
  }

  .card-now-playing {
    position: absolute;
    bottom: 6px;
    right: 6px;
    display: flex;
    gap: 2px;
    align-items: flex-end;
    height: 14px;
    background: rgba(0, 0, 0, 0.6);
    border-radius: 4px;
    padding: 2px 4px;
  }

  .card-now-playing .bar {
    width: 3px;
    background: var(--color-accent);
    border-radius: 1px;
    animation: bar-bounce 0.8s ease-in-out infinite alternate;
  }

  .card-now-playing .bar:nth-child(1) { height: 40%; animation-delay: 0s; }
  .card-now-playing .bar:nth-child(2) { height: 70%; animation-delay: 0.2s; }
  .card-now-playing .bar:nth-child(3) { height: 50%; animation-delay: 0.4s; }

  .card-dl-overlay {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 24px;
    background: rgba(0, 0, 0, 0.75);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .card-dl-bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    background: var(--color-accent-green);
    transition: width 0.3s ease;
  }

  .card-dl-pct {
    font-size: 10px;
    font-weight: 700;
    color: var(--color-accent-green);
    z-index: 1;
  }

  .content-card-footer {
    padding: 6px 8px;
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .content-title {
    flex: 1;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-text);
  }

  .content-card-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .card-action-btn {
    background: none;
    border: none;
    cursor: pointer;
    padding: 2px;
    color: var(--color-text-dim);
    opacity: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast);
  }

  .content-card:hover .card-action-btn,
  .card-action-btn.is-fav,
  .card-action-btn.downloading {
    opacity: 1;
  }

  .card-action-btn:hover {
    color: var(--color-accent);
  }

  .card-action-btn.is-fav {
    color: #e74c3c;
  }

  .card-action-btn svg {
    width: 14px;
    height: 14px;
  }

  /* Series detail page */
  .episodes-section {
    padding: 0 10px 10px;
  }

  /* Season tabs */
  .season-tabs {
    display: flex;
    gap: 2px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    overflow-x: auto;
  }

  .season-tab {
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-muted);
    background: transparent;
    white-space: nowrap;
    transition: all var(--transition-fast);
  }

  .season-tab:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .season-tab.active {
    background: var(--color-accent);
    color: #fff;
  }

  /* Episodes list */
  .episodes-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
  }

  .ep-row {
    display: flex;
    align-items: center;
    border-radius: 8px;
    transition: background var(--transition-fast);
  }

  .ep-row:hover {
    background: var(--color-hover);
  }

  .ep-row.active {
    background: rgba(233, 69, 96, 0.08);
  }

  .ep-row-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
    text-align: left;
    padding: 10px 12px;
    background: transparent;
    color: var(--color-text);
  }

  .ep-num {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-accent);
    width: 36px;
    flex-shrink: 0;
  }

  .ep-title {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-text);
  }

  .error-detail {
    font-size: 12px;
    color: var(--color-accent);
    max-width: 400px;
    word-break: break-word;
    background: rgba(233, 69, 96, 0.08);
    padding: 8px 14px;
    border-radius: 8px;
  }

  .error-actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  /* VOD detail page */
  .vod-detail {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow-y: auto;
  }

  .vod-hero {
    flex-shrink: 0;
    min-height: 360px;
    background-size: cover;
    background-position: center top;
    background-color: var(--color-surface);
    position: relative;
  }

  .vod-hero-overlay {
    display: flex;
    gap: 24px;
    padding: 28px;
    min-height: 360px;
    background: linear-gradient(to top, rgba(0,0,0,0.97) 0%, rgba(0,0,0,0.5) 40%, rgba(0,0,0,0.6) 100%);
    align-items: flex-end;
    position: relative;
  }

  .vod-poster {
    width: 180px;
    aspect-ratio: 2 / 3;
    object-fit: cover;
    border-radius: 12px;
    flex-shrink: 0;
    box-shadow: 0 8px 32px rgba(0,0,0,0.6);
  }

  .vod-meta {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    padding-bottom: 4px;
  }

  .vod-meta h2 {
    font-size: 24px;
    font-weight: 700;
    color: #fff;
    line-height: 1.2;
  }

  .vod-genre {
    font-size: 12px;
    color: rgba(255,255,255,0.6);
  }

  .vod-tags {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .vod-tag {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 4px;
    background: rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.8);
  }

  .vod-director {
    font-size: 12px;
    color: rgba(255,255,255,0.6);
  }

  .vod-director strong {
    color: rgba(255,255,255,0.8);
  }

  .vod-cast {
    font-size: 12px;
    color: rgba(255,255,255,0.5);
  }

  .vod-cast strong {
    color: rgba(255,255,255,0.7);
  }

  .vod-plot {
    font-size: 13px;
    color: rgba(255,255,255,0.75);
    line-height: 1.5;
    max-height: 120px;
    overflow-y: auto;
  }

  .vod-actions {
    display: flex;
    gap: 10px;
    margin-top: 8px;
  }

  .vod-play-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    border-radius: 8px;
    background: var(--color-accent);
    color: #fff;
    font-size: 14px;
    font-weight: 600;
    transition: background var(--transition-fast), transform var(--transition-fast);
  }

  .vod-play-btn:hover {
    background: #c73850;
    transform: scale(1.03);
  }

  .vod-play-btn svg {
    width: 18px;
    height: 18px;
  }

  .vod-dl-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 18px;
    border-radius: 8px;
    background: rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.8);
    font-size: 13px;
    font-weight: 500;
    transition: background var(--transition-fast);
  }

  .vod-dl-btn:hover {
    background: rgba(255,255,255,0.18);
  }

  .vod-dl-btn.downloading {
    color: var(--color-accent-green);
  }

  .vod-dl-btn svg {
    width: 16px;
    height: 16px;
  }

  .vod-close-btn {
    position: absolute;
    top: 12px;
    right: 12px;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: rgba(0,0,0,0.5);
    color: #fff;
    backdrop-filter: blur(4px);
  }

  .vod-close-btn:hover {
    background: rgba(255,255,255,0.2);
  }

  .vod-close-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  .vod-related {
    padding: 20px;
  }

  .vod-related-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 14px;
  }

  .vod-related-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 12px;
  }

  .vod-error-panel {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px 20px;
    flex: 1;
    text-align: center;
  }

  .vod-error-panel p {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
  }
</style>
