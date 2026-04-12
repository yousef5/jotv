<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Hls from 'hls.js';
  import mpegts from 'mpegts.js';
  import PlaylistItem from '$lib/components/PlaylistItem.svelte';
  import AddPlaylistModal from '$lib/components/AddPlaylistModal.svelte';
  import { deletePlaylist, exportPlaylist, refreshPlaylist, getGroupsByType, getRecentlyAdded, getChannelsByGroup, getSeriesInfo, searchChannelsInPlaylist, recordViewing, detectExternalPlayers, launchExternalPlayer, addFavorite, removeFavorite, isFavorite, mpvPlay, mpvLoad, mpvStop, mpvPause, mpvFullscreen, mpvIsRunning, startDownload } from '$lib/tauri';
  import type { Channel, ChannelGroup, SeriesDetail, ExternalPlayer } from '$lib/tauri';
  import {
    playlists, selectedPlaylist, contentTypeCounts,
    loadPlaylists, loadContentTypeCounts,
  } from '$lib/stores/playlists';

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
    loadPlaylists();
    document.addEventListener('fullscreenchange', handleFullscreenChange);
    document.addEventListener('click', closeFavMenu);
    document.addEventListener('keydown', handleGlobalKeydown);
    try { externalPlayers = await detectExternalPlayers(); } catch {}
  });

  onDestroy(() => {
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
    stopPlaying();
    if (p) {
      loadContentTypeCounts(p.id);
      activeTab = 'live';
      loadGroupsForTab(p.id, 'live');
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
    activeTab = tab;
    clearSearch();
    const p = $selectedPlaylist;
    if (p) loadGroupsForTab(p.id, tab);
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
      // VOD/series → external player or browser
      destroyPlayer();
      playingChannel = ch;
      playStartTime = Date.now();
      playerError = '';
      playerLoading = true;
      usingMpv = false;
      requestAnimationFrame(() => {
        if (videoEl) setupStream(ch.stream_url);
      });
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

  function playEpisode(streamUrl: string, title: string) {
    // Create a fake channel to play the episode in the inline player
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
    playerLoading = true;
    requestAnimationFrame(() => {
      if (videoEl) setupStream(streamUrl);
    });
  }

  function getBackdrop(info: SeriesDetail['info']): string | null {
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

  <!-- Column 2: Groups -->
  <div class="col col-groups">
    {#if $selectedPlaylist}
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
    {:else}
      <div class="col-placeholder">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 6h16M4 10h16M4 14h10"/></svg>
        <p>Select a playlist</p>
      </div>
    {/if}
  </div>

  <!-- Column 3: Channels + Inline Player -->
  <div class="col col-channels">
    {#if selectedGroup !== null}
      <!-- Player panel -->
      {#if playingChannel}
        {#if usingMpv}
          <!-- MPV player controls -->
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
        {:else}
          <!-- Browser inline player (VOD/series) -->
          <div class="inline-player" bind:this={playerContainerEl}>
            <div class="player-wrapper">
              <!-- svelte-ignore a11y_media_has_caption -->
              <video
                bind:this={videoEl}
                autoplay playsinline preload="auto"
                onplay={() => { playerPaused = false; playerLoading = false; }}
                onpause={() => { playerPaused = true; }}
                onwaiting={() => { playerLoading = true; }}
                onplaying={() => { playerLoading = false; playerError = ''; retryCount = 0; }}
                onstalled={() => { if (!playerPaused) playerLoading = true; }}
                onerror={() => { if (playingChannel && !hls && !mpegtsPlayer) scheduleRetry(() => { if (playingChannel) setupStream(playingChannel.stream_url); }); }}
                class="player-video"
              ></video>
              {#if playerLoading && !playerError && !vodPlayingExternal}
                <div class="player-overlay"><div class="spinner"></div></div>
              {/if}
              {#if vodPlayingExternal}
                <div class="player-overlay external">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="external-icon"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg>
                  <span class="external-text">Playing in {externalPlayers[0]?.name ?? 'external player'}</span>
                </div>
              {/if}
              {#if playerError}
                <div class="player-overlay"><span class="player-error-text">{playerError}</span></div>
              {/if}
            </div>
            <div class="player-bar">
              <div class="player-info">
                {#if playingChannel.logo_url}
                  <img class="player-ch-icon" src={playingChannel.logo_url} alt="" />
                {/if}
                <span class="player-ch-name">{playingChannel.name}</span>
              </div>
              <div class="player-controls">
                {#if !vodPlayingExternal}
                  <button class="ctrl-btn" onclick={togglePlay} title={playerPaused ? 'Play' : 'Pause'}>
                    {#if playerPaused}
                      <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                    {:else}
                      <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
                    {/if}
                  </button>
                  <button class="ctrl-btn" onclick={toggleFullscreen} title="Fullscreen">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><polyline points="21 3 14 10"/><polyline points="3 21 10 14"/></svg>
                  </button>
                {/if}
                {#if externalPlayers.length > 0}
                  <button class="ctrl-btn" onclick={() => openCurrentInPlayer(0)} title="Open in {externalPlayers[0].name}">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>
                  </button>
                {/if}
                <button class="ctrl-btn" onclick={stopPlaying} title="Close">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                </button>
              </div>
            </div>
          </div>
        {/if}
      {/if}

      <!-- Series detail overlay -->
      {#if viewingSeries && activeTab === 'series'}
        <div class="series-detail">
          {#if seriesLoading}
            <div class="col-empty">Loading series info...</div>
          {:else if seriesDetail}
            {@const backdrop = getBackdrop(seriesDetail.info)}
            <!-- Series hero -->
            <div class="series-hero" style={backdrop ? `background-image: url(${backdrop})` : ''}>
              <div class="series-hero-overlay">
                {#if seriesDetail.info.cover}
                  <img class="series-poster" src={seriesDetail.info.cover} alt="" />
                {/if}
                <div class="series-meta">
                  <h2>{seriesDetail.info.name || viewingSeries.name}</h2>
                  {#if seriesDetail.info.genre}
                    <span class="series-genre">{seriesDetail.info.genre}</span>
                  {/if}
                  <div class="series-tags">
                    {#if seriesDetail.info.rating}
                      <span class="series-tag">&#9733; {seriesDetail.info.rating}</span>
                    {/if}
                    {#if seriesDetail.info.release_date}
                      <span class="series-tag">{seriesDetail.info.release_date}</span>
                    {/if}
                    <span class="series-tag">{seriesDetail.seasons.length} seasons</span>
                  </div>
                  {#if seriesDetail.info.plot}
                    <p class="series-plot">{seriesDetail.info.plot}</p>
                  {/if}
                  {#if seriesDetail.info.cast}
                    <p class="series-cast">{seriesDetail.info.cast}</p>
                  {/if}
                </div>
                <button class="series-close-btn" onclick={closeSeries} title="Back">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                </button>
              </div>
            </div>

            <!-- Season tabs -->
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

            <!-- Episodes list -->
            <div class="col-scroll">
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
          {:else}
            <div class="series-error">
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

      <!-- Regular channel list / series grid -->
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
          {:else if activeTab === 'series'}
            <!-- Series thumbnail grid -->
            <div class="series-grid">
              {#each groupChannels as ch (ch.id)}
                <button class="series-card" onclick={() => openSeries(ch)}>
                  {#if ch.logo_url}
                    <img class="series-thumb" src={ch.logo_url} alt="" loading="lazy" />
                  {:else}
                    <div class="series-thumb placeholder">
                      <span>{ch.name.charAt(0).toUpperCase()}</span>
                    </div>
                  {/if}
                  <span class="series-title">{ch.name}</span>
                </button>
              {/each}
            </div>
            {#if hasMore}
              <button class="load-more-btn" onclick={loadMore} disabled={channelsLoading}>
                {channelsLoading ? 'Loading...' : 'Load More'}
              </button>
            {/if}
          {:else}
            <!-- Live / VOD channel list -->
            {#each groupChannels as ch (ch.id)}
              <div class="channel-row" class:active={playingChannel?.id === ch.id}>
                <button
                  class="channel-item"
                  onclick={() => playChannel(ch)}
                >
                  {#if ch.logo_url}
                    <img class="channel-icon" src={ch.logo_url} alt="" loading="lazy" />
                  {:else}
                    <div class="channel-icon placeholder">
                      <span>{ch.name.charAt(0).toUpperCase()}</span>
                    </div>
                  {/if}
                  <span class="channel-name" title={ch.name}>{ch.name}</span>
                  {#if playingChannel?.id === ch.id}
                    <div class="now-playing">
                      <span class="bar"></span><span class="bar"></span><span class="bar"></span>
                    </div>
                  {/if}
                </button>
                <!-- Download button (VOD only) -->
                {#if activeTab === 'vod'}
                  <button
                    class="dl-btn"
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
                <!-- Favorite button -->
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
                  <!-- Category picker dropdown -->
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
            {/each}
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
  .inline-player {
    flex-shrink: 0;
    border-bottom: 1px solid var(--color-border);
    background: #000;
  }

  .inline-player:fullscreen {
    display: flex;
    flex-direction: column;
  }

  .inline-player:fullscreen .player-wrapper {
    flex: 1;
    max-height: none;
    aspect-ratio: auto;
  }

  .player-wrapper {
    width: 100%;
    aspect-ratio: 16 / 9;
    max-height: 280px;
    background: #000;
    position: relative;
  }

  .player-video {
    width: 100%;
    height: 100%;
    object-fit: contain;
    background: #000;
  }

  .player-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }

  .spinner {
    width: 36px;
    height: 36px;
    border: 3px solid rgba(255, 255, 255, 0.15);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .player-error-text {
    font-size: 13px;
    color: var(--color-accent);
    background: rgba(0, 0, 0, 0.6);
    padding: 6px 14px;
    border-radius: 8px;
  }

  .player-overlay.external {
    flex-direction: column;
    gap: 8px;
    background: rgba(0, 0, 0, 0.7);
    pointer-events: none;
  }

  .external-icon {
    width: 40px;
    height: 40px;
    color: var(--color-accent-green);
    opacity: 0.8;
  }

  .external-text {
    font-size: 13px;
    color: rgba(255,255,255,0.7);
  }

  .player-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: var(--color-card);
    gap: 8px;
  }

  .player-info {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .player-ch-icon {
    width: 24px;
    height: 24px;
    border-radius: 4px;
    object-fit: contain;
    background: var(--color-surface);
    flex-shrink: 0;
  }

  .player-ch-name {
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .player-controls {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .ctrl-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    transition: all var(--transition-fast);
  }

  .ctrl-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .ctrl-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  .channel-total {
    font-size: 12px;
    color: var(--color-text-muted);
    background: var(--color-surface);
    padding: 2px 8px;
    border-radius: 8px;
    flex-shrink: 0;
  }

  .channel-row {
    display: flex;
    align-items: center;
    border-radius: 8px;
    margin-bottom: 1px;
    transition: background var(--transition-fast);
  }

  .channel-row:hover {
    background: var(--color-hover);
  }

  .channel-row.active {
    background: rgba(233, 69, 96, 0.08);
  }

  .channel-item {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
    text-align: left;
    padding: 6px 10px;
    background: transparent;
    color: var(--color-text);
  }

  /* Favorite button */
  /* Download button */
  .dl-btn {
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
  }

  .channel-row:hover .dl-btn { opacity: 1; }
  .dl-btn.downloading { opacity: 1; color: var(--color-accent-green); }
  .dl-btn:hover { background: var(--color-surface); color: var(--color-accent-green); }
  .dl-btn :global(svg) { width: 15px; height: 15px; }
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

  .channel-row:hover .fav-btn,
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

  .channel-icon {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    object-fit: contain;
    background: var(--color-surface);
    flex-shrink: 0;
  }

  .channel-icon.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .channel-icon.placeholder span {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-accent);
    opacity: 0.5;
  }

  .channel-name {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
  .series-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 12px;
    padding: 8px;
  }

  .series-card {
    display: flex;
    flex-direction: column;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    overflow: hidden;
    text-align: left;
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .series-card:hover {
    transform: translateY(-3px);
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.35);
  }

  .series-thumb {
    width: 100%;
    aspect-ratio: 2 / 3;
    object-fit: cover;
    background: var(--color-surface);
  }

  .series-thumb.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .series-thumb.placeholder span {
    font-size: 32px;
    font-weight: 700;
    color: var(--color-accent);
    opacity: 0.3;
  }

  .series-title {
    padding: 6px 8px;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-text);
  }

  /* Series detail page */
  .series-detail {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow: hidden;
  }

  .series-hero {
    flex-shrink: 0;
    min-height: 280px;
    max-height: 360px;
    background-size: cover;
    background-position: center top;
    background-color: var(--color-surface);
    position: relative;
  }

  .series-hero-overlay {
    display: flex;
    gap: 20px;
    padding: 24px;
    height: 100%;
    background: linear-gradient(to top, rgba(0,0,0,0.95) 0%, rgba(0,0,0,0.4) 50%, rgba(0,0,0,0.6) 100%);
    align-items: flex-end;
    position: relative;
  }

  .series-poster {
    width: 140px;
    aspect-ratio: 2 / 3;
    object-fit: cover;
    border-radius: 10px;
    flex-shrink: 0;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
  }

  .series-meta {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    padding-bottom: 4px;
  }

  .series-meta h2 {
    font-size: 22px;
    font-weight: 700;
    color: #fff;
    line-height: 1.2;
  }

  .series-genre {
    font-size: 12px;
    color: rgba(255,255,255,0.6);
  }

  .series-tags {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .series-tag {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 4px;
    background: rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.8);
  }

  .series-plot {
    font-size: 12px;
    color: rgba(255,255,255,0.7);
    line-height: 1.4;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
  }

  .series-cast {
    font-size: 11px;
    color: rgba(255,255,255,0.5);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .series-close-btn {
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

  .series-close-btn:hover {
    background: rgba(255,255,255,0.2);
  }

  .series-close-btn :global(svg) {
    width: 16px;
    height: 16px;
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

  .series-error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px 20px;
    flex: 1;
    text-align: center;
  }

  .series-error p {
    font-size: 15px;
    font-weight: 600;
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
</style>
