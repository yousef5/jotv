<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Hls from 'hls.js';
  import mpegts from 'mpegts.js';
  import { favorites, loadFavorites } from '$lib/stores/favorites';
  import { removeFavorite, recordViewing, detectExternalPlayers, launchExternalPlayer, mpvPlay, mpvLoad, mpvStop, mpvPause, mpvFullscreen, mpvIsRunning } from '$lib/tauri';
  import type { FavoriteChannel, ExternalPlayer } from '$lib/tauri';

  // Categories
  let selectedCategory = $state<string | null>(null);

  let categories = $derived(() => {
    const map = new Map<string, number>();
    for (const f of $favorites) {
      map.set(f.category, (map.get(f.category) || 0) + 1);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  let filteredFavorites = $derived(() => {
    if (!selectedCategory) return $favorites;
    return $favorites.filter(f => f.category === selectedCategory);
  });

  // Player
  let playingChannel = $state<FavoriteChannel | null>(null);
  let playStartTime = $state<number | null>(null);
  let videoEl = $state<HTMLVideoElement | undefined>();
  let playerContainerEl = $state<HTMLDivElement | undefined>();
  let hls: Hls | null = null;
  let mpegtsPlayer: mpegts.Player | null = null;
  let playerLoading = $state(false);
  let playerPaused = $state(false);
  let playerError = $state('');
  let isFullscreen = $state(false);
  let vodPlayingExternal = $state(false);
  let externalPlayers = $state<ExternalPlayer[]>([]);
  let retryCount = 0;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  const MAX_RETRIES = 8;
  let hlsRecoveryCount = 0;

  const BROWSER_PLAYABLE = ['.mp4', '.webm', '.ogg'];
  type StreamType = 'hls' | 'ts' | 'direct';

  function detectStreamType(url: string): StreamType {
    const lower = url.toLowerCase();
    if (lower.endsWith('.m3u8') || lower.includes('.m3u8')) return 'hls';
    if (lower.endsWith('.ts')) return 'ts';
    return 'direct';
  }

  function isBrowserPlayable(url: string): boolean {
    return BROWSER_PLAYABLE.some(ext => url.toLowerCase().endsWith(ext));
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
  }

  function setupHls(url: string) {
    if (!videoEl) return;
    hls = new Hls({
      enableWorker: true, maxBufferLength: 60, maxMaxBufferLength: 120,
      maxBufferSize: 120_000_000, maxBufferHole: 0.3, backBufferLength: 60,
      liveSyncDurationCount: 3, liveMaxLatencyDurationCount: 10, liveDurationInfinity: true,
      startLevel: -1, abrEwmaDefaultEstimate: 8_000_000, startFragPrefetch: true,
      progressive: true, fragLoadingMaxRetry: 10, fragLoadingRetryDelay: 300,
      manifestLoadingMaxRetry: 10, manifestLoadingRetryDelay: 300,
      levelLoadingMaxRetry: 10, levelLoadingRetryDelay: 300, capLevelOnFPSDrop: true,
    });
    hls.loadSource(url);
    hls.attachMedia(videoEl);
    hls.on(Hls.Events.MANIFEST_PARSED, () => { playerLoading = false; videoEl?.play().catch(() => {}); });
    hls.on(Hls.Events.FRAG_LOADED, () => { playerLoading = false; if (playerError) playerError = ''; retryCount = 0; });
    hls.on(Hls.Events.ERROR, (_e, data) => {
      if (!data.fatal) return;
      if (data.type === Hls.ErrorTypes.NETWORK_ERROR) { hls?.startLoad(); scheduleRetry(() => { hls?.startLoad(); }); }
      else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) { hlsRecoveryCount++; if (hlsRecoveryCount <= 2) hls?.recoverMediaError(); else { hls?.swapAudioCodec(); hls?.recoverMediaError(); hlsRecoveryCount = 0; } }
      else scheduleRetry(() => { if (playingChannel) setupStream(playingChannel.stream_url); });
    });
  }

  function setupMpegts(url: string) {
    if (!videoEl) return;
    mpegtsPlayer = mpegts.createPlayer({ type: 'mpegts', isLive: true, url }, {
      enableWorker: true, enableStashBuffer: true, stashInitialSize: 1024 * 1024,
      autoCleanupSourceBuffer: true, autoCleanupMaxBackwardDuration: 60,
      fixAudioTimestampGap: true, lazyLoad: true, lazyLoadMaxDuration: 120,
    });
    mpegtsPlayer.attachMediaElement(videoEl);
    mpegtsPlayer.load();
    videoEl.play().catch(() => {});
    mpegtsPlayer.on(mpegts.Events.ERROR, () => { scheduleRetry(() => { if (playingChannel) setupStream(playingChannel.stream_url); }); });
    mpegtsPlayer.on(mpegts.Events.STATISTICS_INFO, () => { if (playerLoading) playerLoading = false; retryCount = 0; });
  }

  function scheduleRetry(action: () => void) {
    if (retryTimer) clearTimeout(retryTimer);
    retryCount++;
    if (retryCount <= MAX_RETRIES) {
      const delay = retryCount <= 2 ? retryCount * 250 : Math.min(1000 * Math.pow(2, retryCount - 3), 16000);
      playerError = ''; playerLoading = true;
      retryTimer = setTimeout(action, delay);
    } else { playerLoading = false; playerError = 'Stream unavailable'; }
  }

  function destroyPlayer() {
    if (retryTimer) { clearTimeout(retryTimer); retryTimer = null; }
    if (hls) { hls.destroy(); hls = null; }
    if (mpegtsPlayer) { try { mpegtsPlayer.pause(); mpegtsPlayer.unload(); mpegtsPlayer.detachMediaElement(); mpegtsPlayer.destroy(); } catch {} mpegtsPlayer = null; }
    if (videoEl) videoEl.removeAttribute('src');
  }

  async function openInExternalPlayer(url: string) {
    playerLoading = false; vodPlayingExternal = true;
    if (externalPlayers.length === 0) { playerError = 'No external player found. Install VLC or MPV.'; vodPlayingExternal = false; return; }
    try { await launchExternalPlayer(externalPlayers[0].path, url); } catch (e) { playerError = `Failed: ${e}`; vodPlayingExternal = false; }
  }

  async function openCurrentInPlayer() {
    if (!playingChannel || !externalPlayers[0]) return;
    try { await launchExternalPlayer(externalPlayers[0].path, playingChannel.stream_url); vodPlayingExternal = true; } catch (e) { playerError = `Failed: ${e}`; }
  }

  let usingMpv = $state(false);

  async function playFav(fav: FavoriteChannel) {
    if (playingChannel && playStartTime) {
      const d = Math.floor((Date.now() - playStartTime) / 1000);
      if (d > 5) recordViewing(fav.channel_id, d).catch(() => {});
    }

    const type = detectStreamType(fav.stream_url);
    const isLive = type === 'hls' || type === 'ts';

    if (isLive) {
      // Live → MPV
      destroyPlayer();
      playingChannel = fav;
      playStartTime = Date.now();
      playerError = ''; playerLoading = true; usingMpv = true; vodPlayingExternal = false;
      try {
        const running = await mpvIsRunning();
        if (running) await mpvLoad(fav.stream_url, fav.channel_name);
        else await mpvPlay(fav.stream_url, fav.channel_name);
        playerLoading = false;
      } catch (e) {
        playerError = `MPV: ${e}`;
        playerLoading = false;
        usingMpv = false;
      }
    } else {
      // VOD → browser/external
      destroyPlayer();
      playingChannel = fav;
      playStartTime = Date.now();
      playerError = ''; playerLoading = true; usingMpv = false; vodPlayingExternal = false;
      requestAnimationFrame(() => { if (videoEl) setupStream(fav.stream_url); });
    }
  }

  async function stopPlaying() {
    if (playingChannel && playStartTime) {
      const d = Math.floor((Date.now() - playStartTime) / 1000);
      if (d > 5) try { await recordViewing(playingChannel.channel_id, d); } catch {}
    }
    if (usingMpv) { mpvStop().catch(() => {}); usingMpv = false; }
    destroyPlayer();
    playingChannel = null; playStartTime = null;
    playerLoading = false; playerPaused = false; playerError = ''; vodPlayingExternal = false;
    if (document.fullscreenElement) document.exitFullscreen();
  }

  function togglePlay() { if (!videoEl) return; if (videoEl.paused) videoEl.play().catch(() => {}); else videoEl.pause(); }
  function toggleFullscreen() { if (!playerContainerEl) return; if (document.fullscreenElement) document.exitFullscreen(); else playerContainerEl.requestFullscreen(); }
  function handleFullscreenChange() { isFullscreen = !!document.fullscreenElement; }

  async function handleRemove(fav: FavoriteChannel) {
    const wasPlaying = playingChannel?.channel_id === fav.channel_id;
    if (wasPlaying) await stopPlaying();
    try { await removeFavorite(fav.channel_id); await loadFavorites(); } catch {}
  }

  function getCategoryIcon(cat: string): string {
    const map: Record<string, string> = {
      'News': 'M19 20H5a2 2 0 01-2-2V6a2 2 0 012-2h10a2 2 0 012 2v1M19 20a2 2 0 002-2V9a2 2 0 00-2-2h-2M19 20l-7-5',
      'Sports': 'M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zM12 2v20M2 12h20',
      'Movies': 'M7 2v20M17 2v20M2 12h20M2 7h5M2 17h5M17 7h5M17 17h5M2 2h20v20H2z',
      'Kids': 'M12 2a10 10 0 1010 10A10 10 0 0012 2zM8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01',
      'Music': 'M9 18V5l12-2v13M9 18a3 3 0 11-6 0 3 3 0 016 0zM21 16a3 3 0 11-6 0 3 3 0 016 0z',
      'Documentary': 'M2 3h6a4 4 0 014 4v14a3 3 0 00-3-3H2zM22 3h-6a4 4 0 00-4 4v14a3 3 0 013-3h7z',
      'Entertainment': 'M5 3l14 9-14 9V3z',
    };
    return map[cat] || 'M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z';
  }

  onMount(async () => {
    loadFavorites();
    document.addEventListener('fullscreenchange', handleFullscreenChange);
    try { externalPlayers = await detectExternalPlayers(); } catch {}
  });

  onDestroy(() => {
    destroyPlayer();
    if (usingMpv) mpvStop().catch(() => {});
    document.removeEventListener('fullscreenchange', handleFullscreenChange);
  });
</script>

<div class="fav-page fade-in">
  <!-- Column 1: Categories -->
  <div class="col col-cats">
    <div class="col-header">
      <h2>Favorites</h2>
      <span class="total-badge">{$favorites.length}</span>
    </div>
    <div class="col-scroll">
      <button
        class="cat-item"
        class:active={selectedCategory === null}
        onclick={() => selectedCategory = null}
      >
        <div class="cat-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
        </div>
        <span class="cat-name">All</span>
        <span class="cat-count">{$favorites.length}</span>
      </button>
      {#each categories() as [cat, count] (cat)}
        <button
          class="cat-item"
          class:active={selectedCategory === cat}
          onclick={() => selectedCategory = cat}
        >
          <div class="cat-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d={getCategoryIcon(cat)}/></svg>
          </div>
          <span class="cat-name">{cat}</span>
          <span class="cat-count">{count}</span>
        </button>
      {/each}
      {#if $favorites.length === 0}
        <div class="col-empty">No favorites yet</div>
      {/if}
    </div>
  </div>

  <!-- Column 2: Channels list -->
  <div class="col col-channels">
    <div class="col-header">
      <h2 class="truncate">{selectedCategory ?? 'All Favorites'}</h2>
      <span class="count-badge">{filteredFavorites().length}</span>
    </div>
    <div class="col-scroll">
      {#if filteredFavorites().length === 0}
        <div class="col-empty">No channels in this category</div>
      {:else}
        {#each filteredFavorites() as fav (fav.id)}
          <div class="ch-row" class:active={playingChannel?.channel_id === fav.channel_id}>
            <button class="ch-btn" onclick={() => playFav(fav)}>
              {#if fav.logo_url}
                <img class="ch-icon" src={fav.logo_url} alt="" loading="lazy" />
              {:else}
                <div class="ch-icon placeholder">
                  <span>{fav.channel_name.charAt(0).toUpperCase()}</span>
                </div>
              {/if}
              <div class="ch-text">
                <span class="ch-name">{fav.channel_name}</span>
                <span class="ch-meta">{fav.group_name}</span>
              </div>
              {#if playingChannel?.channel_id === fav.channel_id}
                <div class="now-playing">
                  <span class="bar"></span><span class="bar"></span><span class="bar"></span>
                </div>
              {/if}
            </button>
            <button class="remove-btn" onclick={() => handleRemove(fav)} title="Remove">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
          </div>
        {/each}
      {/if}
    </div>
  </div>

  <!-- Column 3: Player -->
  <div class="col col-player">
    {#if playingChannel}
      {#if usingMpv}
        <!-- MPV player panel -->
        <div class="mpv-panel">
          <div class="mpv-display">
            {#if playingChannel.logo_url}
              <img class="mpv-logo" src={playingChannel.logo_url} alt="" />
            {:else}
              <div class="mpv-logo placeholder">
                <span>{playingChannel.channel_name.charAt(0).toUpperCase()}</span>
              </div>
            {/if}
            <div class="mpv-info">
              <h3>{playingChannel.channel_name}</h3>
              <span class="mpv-group">{playingChannel.group_name} &middot; {playingChannel.category}</span>
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
        <!-- Browser inline player (VOD) -->
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
              class="player-video"
            ></video>
            {#if playerLoading && !playerError && !vodPlayingExternal}
              <div class="player-overlay"><div class="spinner"></div></div>
            {/if}
            {#if vodPlayingExternal}
              <div class="player-overlay external">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="ext-icon"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg>
                <span class="ext-text">Playing in {externalPlayers[0]?.name ?? 'external player'}</span>
              </div>
            {/if}
            {#if playerError}
              <div class="player-overlay"><span class="player-error">{playerError}</span></div>
            {/if}
          </div>
          <div class="player-bar">
            <div class="player-info">
              {#if playingChannel.logo_url}
                <img class="player-ch-icon" src={playingChannel.logo_url} alt="" />
              {/if}
              <span class="player-ch-name">{playingChannel.channel_name}</span>
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
                <button class="ctrl-btn" onclick={openCurrentInPlayer} title="Open in {externalPlayers[0].name}">
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

      <!-- Now playing info -->
      <div class="now-info">
        {#if playingChannel.logo_url}
          <img class="now-logo" src={playingChannel.logo_url} alt="" />
        {/if}
        <div class="now-text">
          <h3>{playingChannel.channel_name}</h3>
          <span>{playingChannel.group_name} &middot; {playingChannel.category}</span>
        </div>
      </div>
    {:else}
      <div class="col-placeholder">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
        <p>Select a channel to play</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .fav-page {
    display: grid;
    grid-template-columns: 240px 320px 1fr;
    height: calc(100vh - 48px);
    margin: -24px;
    background: var(--color-base);
  }

  .truncate { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .col {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-right: 1px solid var(--color-border);
  }
  .col:last-child { border-right: none; }

  .col-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 14px 12px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }
  .col-header h2 { font-size: 15px; font-weight: 700; }

  .col-scroll { flex: 1; overflow-y: auto; padding: 6px; }
  .col-empty { display: flex; align-items: center; justify-content: center; height: 80px; color: var(--color-text-muted); font-size: 13px; }
  .col-placeholder { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: var(--color-text-muted); }
  .col-placeholder :global(svg) { width: 40px; height: 40px; opacity: 0.2; }
  .col-placeholder p { font-size: 13px; }

  .total-badge, .count-badge {
    font-size: 11px; color: var(--color-text-muted); background: var(--color-surface);
    padding: 2px 8px; border-radius: 8px; flex-shrink: 0;
  }

  /* Column 1 - Categories */
  .cat-item {
    display: flex; align-items: center; gap: 10px; width: 100%; text-align: left;
    padding: 8px 10px; border-radius: 8px; background: transparent;
    color: var(--color-text); font-size: 13px; margin-bottom: 2px;
    transition: background var(--transition-fast);
  }
  .cat-item:hover { background: var(--color-hover); }
  .cat-item.active { background: var(--color-surface); border: 1px solid var(--color-border); }

  .cat-icon {
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    background: var(--color-surface); border-radius: 8px; color: var(--color-text-muted); flex-shrink: 0;
  }
  .cat-item.active .cat-icon { background: rgba(233, 69, 96, 0.12); color: var(--color-accent); }
  .cat-icon :global(svg) { width: 16px; height: 16px; }
  .cat-name { flex: 1; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cat-count { font-size: 11px; color: var(--color-text-muted); background: var(--color-surface); padding: 1px 7px; border-radius: 8px; flex-shrink: 0; }

  /* Column 2 - Channel list */
  .ch-row {
    display: flex; align-items: center; border-radius: 8px; margin-bottom: 1px;
    transition: background var(--transition-fast);
  }
  .ch-row:hover { background: var(--color-hover); }
  .ch-row.active { background: rgba(233, 69, 96, 0.08); }

  .ch-btn {
    display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0;
    text-align: left; padding: 6px 10px; background: transparent; color: var(--color-text);
  }

  .ch-icon { width: 36px; height: 36px; border-radius: 8px; object-fit: contain; background: var(--color-surface); flex-shrink: 0; }
  .ch-icon.placeholder { display: flex; align-items: center; justify-content: center; }
  .ch-icon.placeholder span { font-size: 14px; font-weight: 700; color: var(--color-accent); opacity: 0.5; }

  .ch-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .ch-name { font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ch-meta { font-size: 11px; color: var(--color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .remove-btn {
    width: 28px; height: 28px; display: flex; align-items: center; justify-content: center;
    border-radius: 6px; background: transparent; color: var(--color-text-muted);
    opacity: 0; transition: all var(--transition-fast); flex-shrink: 0; margin-right: 6px;
  }
  .ch-row:hover .remove-btn { opacity: 1; }
  .remove-btn:hover { background: rgba(233, 69, 96, 0.1); color: var(--color-accent); }
  .remove-btn :global(svg) { width: 14px; height: 14px; }

  .now-playing { display: flex; align-items: flex-end; gap: 2px; height: 14px; flex-shrink: 0; }
  .now-playing .bar { width: 3px; background: var(--color-accent); border-radius: 1px; animation: bars 0.8s ease-in-out infinite alternate; }
  .now-playing .bar:nth-child(1) { height: 40%; animation-delay: 0s; }
  .now-playing .bar:nth-child(2) { height: 70%; animation-delay: 0.2s; }
  .now-playing .bar:nth-child(3) { height: 50%; animation-delay: 0.4s; }
  @keyframes bars { 0% { height: 30%; } 100% { height: 100%; } }

  /* MPV panel */
  .mpv-panel { flex-shrink: 0; border-bottom: 1px solid var(--color-border); padding: 20px; background: linear-gradient(135deg, var(--color-card), var(--color-surface)); display: flex; flex-direction: column; gap: 16px; }
  .mpv-display { display: flex; align-items: center; gap: 16px; }
  .mpv-logo { width: 64px; height: 64px; border-radius: 12px; object-fit: contain; background: var(--color-surface); flex-shrink: 0; }
  .mpv-logo.placeholder { display: flex; align-items: center; justify-content: center; }
  .mpv-logo.placeholder span { font-size: 24px; font-weight: 700; color: var(--color-accent); opacity: 0.5; }
  .mpv-info { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .mpv-info h3 { font-size: 16px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mpv-group { font-size: 12px; color: var(--color-text-muted); }
  .mpv-status { font-size: 11px; font-weight: 600; }
  .mpv-status.live { color: var(--color-accent-green); }
  .mpv-status.loading { color: var(--color-accent-yellow); animation: pulse 1.5s ease-in-out infinite; }
  .mpv-status.error { color: var(--color-accent); }
  .mpv-controls { display: flex; gap: 8px; }
  .mpv-btn { height: 36px; padding: 0 16px; display: flex; align-items: center; justify-content: center; border-radius: 8px; background: var(--color-surface); color: var(--color-text); font-size: 12px; transition: all var(--transition-fast); }
  .mpv-btn:hover { background: var(--color-hover); }
  .mpv-btn.stop { color: var(--color-accent); }
  .mpv-btn.stop:hover { background: rgba(233, 69, 96, 0.1); }
  .mpv-btn :global(svg) { width: 16px; height: 16px; }

  /* Column 3 - Player */
  .inline-player { flex-shrink: 0; border-bottom: 1px solid var(--color-border); background: #000; }
  .inline-player:fullscreen { display: flex; flex-direction: column; }
  .inline-player:fullscreen .player-wrapper { flex: 1; max-height: none; aspect-ratio: auto; }

  .player-wrapper { width: 100%; aspect-ratio: 16 / 9; background: #000; position: relative; }
  .player-video { width: 100%; height: 100%; object-fit: contain; background: #000; }

  .player-overlay { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,0.5); pointer-events: none; }
  .player-overlay.external { flex-direction: column; gap: 8px; background: rgba(0,0,0,0.7); }
  .ext-icon { width: 40px; height: 40px; color: var(--color-accent-green); opacity: 0.8; }
  .ext-text { font-size: 13px; color: rgba(255,255,255,0.7); }
  .spinner { width: 36px; height: 36px; border: 3px solid rgba(255,255,255,0.15); border-top-color: var(--color-accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .player-error { font-size: 13px; color: var(--color-accent); background: rgba(0,0,0,0.6); padding: 6px 14px; border-radius: 8px; }

  .player-bar { display: flex; align-items: center; justify-content: space-between; padding: 6px 12px; background: var(--color-card); gap: 8px; }
  .player-info { display: flex; align-items: center; gap: 8px; min-width: 0; flex: 1; }
  .player-ch-icon { width: 24px; height: 24px; border-radius: 4px; object-fit: contain; background: var(--color-surface); flex-shrink: 0; }
  .player-ch-name { font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .player-controls { display: flex; align-items: center; gap: 2px; flex-shrink: 0; }

  .ctrl-btn { width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; border-radius: 6px; background: transparent; color: var(--color-text-muted); transition: all var(--transition-fast); }
  .ctrl-btn:hover { background: var(--color-hover); color: var(--color-text); }
  .ctrl-btn :global(svg) { width: 16px; height: 16px; }

  .now-info {
    display: flex; align-items: center; gap: 14px; padding: 16px 20px;
    border-bottom: 1px solid var(--color-border);
  }
  .now-logo { width: 48px; height: 48px; border-radius: 10px; object-fit: contain; background: var(--color-surface); flex-shrink: 0; }
  .now-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .now-text h3 { font-size: 15px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .now-text span { font-size: 12px; color: var(--color-text-muted); }
</style>
