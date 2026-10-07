<script lang="ts" module>
  import type { WatchRequest } from '$lib/stores/watch';

  // Kept across navigation so coming back from the mini player resumes the same queue
  let lastRequest: WatchRequest | null = null;
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { fade } from 'svelte/transition';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { getChannel, getSeriesInfo, mpvPause, setSetting } from '$lib/tauri';
  import { playlists, loadPlaylists } from '$lib/stores/playlists';
  import {
    nowPlaying, liveStatus, liveError, liveFullscreen, videoAnchor, liveVolume, liveMuted, playbackPos,
    playMedia, stopLive, setVolume, changeVolume, toggleMute, seekBy, seekTo,
  } from '$lib/stores/live';
  import { pendingWatch, watchHref } from '$lib/stores/watch';
  import type { WatchItem } from '$lib/stores/watch';
  import { splitTitle, remoteIdFromUrl } from '$lib/playback';
  import TrackPanel from '$lib/components/TrackPanel.svelte';

  const AUTO_NEXT_SECONDS = 8;

  let req = $state<WatchRequest | null>(null);
  let error = $state('');
  let screenEl = $state<HTMLDivElement | undefined>();
  let now = $state(performance.now());
  let dragging = $state<number | null>(null);
  let countdown = $state<number | null>(null);
  let countdownTimer: ReturnType<typeof setInterval> | undefined;
  let showHint = $state(false);
  let showTracks = $state(false);

  let isSeries = $derived(req?.channel.content_type === 'series');
  let item = $derived(req ? req.items[req.index] : null);
  let next = $derived(req && req.index + 1 < req.items.length ? req.items[req.index + 1] : null);
  let mine = $derived(!!item && $nowPlaying?.kind === 'vod' && $nowPlaying.url === item.url);
  let playing = $derived(mine && $liveStatus === 'live');
  let starting = $derived(!mine || $liveStatus === 'starting' || $liveStatus === 'buffering');
  let duration = $derived(mine ? $playbackPos.duration : 0);
  // Position advances between status polls while playing
  let position = $derived.by(() => {
    if (!mine) return 0;
    const p = $playbackPos;
    const live = playing ? (now - p.at) / 1000 : 0;
    return Math.min(p.duration || Infinity, p.pos + Math.max(0, live));
  });
  let shownPos = $derived(dragging ?? position);

  $effect(() => {
    videoAnchor.set(screenEl ? { el: screenEl, kind: 'vod' } : null);
    return () => videoAnchor.set(null);
  });

  // Next episode countdown when one ends
  $effect(() => {
    if (mine && $playbackPos.eof && next && countdown === null) startCountdown();
  });

  onMount(() => {
    init();
    let frame = 0;
    const tick = () => {
      now = performance.now();
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      clearInterval(countdownTimer);
    };
  });

  async function init() {
    const id = parseInt($page.url.searchParams.get('id') ?? '');
    const ep = parseInt($page.url.searchParams.get('ep') ?? '');
    if (isNaN(id)) { error = 'Nothing to play.'; return; }

    const pending = get(pendingWatch);
    pendingWatch.set(null);
    let r: WatchRequest | null = pending?.channel.id === id ? pending : null;
    if (!r && lastRequest?.channel.id === id) {
      const i = isNaN(ep) ? lastRequest.index : lastRequest.items.findIndex((x) => x.id === ep);
      if (i >= 0) r = { ...lastRequest, index: i };
    }
    if (!r) {
      try {
        r = await buildRequest(id, isNaN(ep) ? null : ep);
      } catch (e) {
        error = String(e);
        return;
      }
    }
    req = r;
    lastRequest = r;
    await play(r.fromStart);
  }

  /** Rebuilds the queue when opened directly (no hand-off from the title page). */
  async function buildRequest(id: number, ep: number | null): Promise<WatchRequest> {
    const channel = await getChannel(id);
    if (channel.content_type === 'vod') {
      return {
        channel,
        poster: channel.logo_url,
        index: 0,
        items: [{ id, url: channel.stream_url, title: splitTitle(channel.name).title, progressKey: `progress_vod_${id}` }],
      };
    }
    if (!$playlists.length) await loadPlaylists();
    const p = $playlists.find((x) => x.id === channel.playlist_id);
    const remote = remoteIdFromUrl(channel.stream_url);
    if (!p?.source_url || !p.xtream_username || !p.xtream_password || remote === null) {
      throw new Error('This series can only be opened from its page.');
    }
    const detail = await getSeriesInfo(p.source_url, p.xtream_username, p.xtream_password, remote);
    const title = splitTitle(channel.name).title;
    const items: WatchItem[] = detail.seasons.flatMap((s) =>
      s.episodes.map((e) => ({
        id: e.id,
        url: e.stream_url,
        title,
        subtitle: `S${s.season_number}:E${e.episode_num}`,
        progressKey: `progress_ep_${e.id}`,
        season: s.season_number,
        episode: e.episode_num,
      })),
    );
    if (!items.length) throw new Error('No episodes found for this series.');
    const index = Math.max(0, ep === null ? 0 : items.findIndex((x) => x.id === ep));
    return { channel, poster: channel.logo_url, items, index };
  }

  async function play(fromStart = false) {
    if (!req || !item) return;
    cancelCountdown();
    if (mine) return; // already playing (back from the mini player)
    if (isSeries && item.season && item.episode) {
      setSetting(`series_progress_${req.channel.id}`, `${item.season}:${item.episode}`).catch(() => {});
    }
    await playMedia(
      {
        channel: req.channel,
        title: item.title,
        subtitle: item.subtitle,
        url: item.url,
        poster: req.poster,
        progressKey: item.progressKey,
        href: watchHref(req.channel.id, item, isSeries),
      },
      { fromStart },
    );
  }

  async function playNext() {
    if (!req || !next) return;
    req = { ...req, index: req.index + 1, fromStart: false };
    lastRequest = req;
    goto(watchHref(req.channel.id, req.items[req.index], true), { replaceState: true, noScroll: true, keepFocus: true });
    await play();
  }

  function startCountdown() {
    countdown = AUTO_NEXT_SECONDS;
    clearInterval(countdownTimer);
    countdownTimer = setInterval(() => {
      if (countdown === null) return;
      countdown -= 1;
      if (countdown <= 0) playNext();
    }, 1000);
  }

  function cancelCountdown() {
    clearInterval(countdownTimer);
    countdown = null;
  }

  function back() {
    if ($liveFullscreen) return liveFullscreen.set(false);
    if (history.length > 1) history.back();
    else goto(req ? `/title?id=${req.channel.id}` : '/');
  }

  async function close() {
    await stopLive();
    back();
  }

  function fmt(secs: number): string {
    if (!isFinite(secs) || secs < 0) secs = 0;
    const s = Math.floor(secs % 60);
    const m = Math.floor((secs / 60) % 60);
    const h = Math.floor(secs / 3600);
    const mm = h ? String(m).padStart(2, '0') : String(m);
    return `${h ? `${h}:` : ''}${mm}:${String(s).padStart(2, '0')}`;
  }

  function onKey(e: KeyboardEvent) {
    if ($liveFullscreen) return; // VideoSurface handles keys in fullscreen
    if (e.target instanceof HTMLInputElement && e.target.type !== 'range') return;
    const k = e.key;
    if (k === ' ' || k === 'k') { e.preventDefault(); mpvPause().catch(() => {}); }
    else if (k === 'ArrowLeft' || k === 'j') { e.preventDefault(); seekBy(-10); }
    else if (k === 'ArrowRight' || k === 'l') { e.preventDefault(); seekBy(10); }
    else if (k === 'ArrowUp') { e.preventDefault(); changeVolume(5); }
    else if (k === 'ArrowDown') { e.preventDefault(); changeVolume(-5); }
    else if (k === 'm') toggleMute();
    else if (k === 'f') liveFullscreen.set(true);
    else if (k === 'n' && next) playNext();
    else if (k === 'Escape') back();
    else if (k === '?') showHint = !showHint;
    else if (k === 'c') showTracks = !showTracks;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="watch">
  <header class="top">
    <button class="ib" onclick={back} aria-label="Back" title="Back · Esc">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
    </button>
    {#if item}
      <div class="titles">
        <h1 dir="auto">{item.title}</h1>
        {#if item.subtitle}<p>{item.subtitle}</p>{/if}
      </div>
    {/if}
    <span class="grow"></span>
    <button class="ib" onclick={close} aria-label="Stop and close" title="Stop and close">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
    </button>
  </header>

  <!-- MPV's native surface is placed exactly over this box -->
  <div class="screen" bind:this={screenEl}>
    {#if req?.poster}<img class="screen-bg" src={req.poster} alt="" aria-hidden="true" />{/if}
    {#if error}
      <div class="screen-msg">
        <strong>Can't play this</strong>
        <span>{error}</span>
        <button class="btn" onclick={back}>Go back</button>
      </div>
    {:else if starting || !$nowPlaying?.embedded}
      <div class="screen-msg">
        {#if mine && !$nowPlaying?.embedded}
          <strong>Playing in a separate MPV window</strong>
        {:else}
          <span class="spinner" aria-label="Loading"></span>
        {/if}
      </div>
    {/if}
  </div>

  <footer class="controls">
    {#if countdown !== null && next}
      <div class="upnext" transition:fade={{ duration: 150 }}>
        <span>Next episode <b>{next.subtitle}</b> in {countdown}s</span>
        <button class="btn" onclick={playNext}>Play now</button>
        <button class="btn ghost" onclick={cancelCountdown}>Cancel</button>
      </div>
    {/if}

    <div class="seek">
      <span class="time">{fmt(shownPos)}</span>
      <input
        type="range"
        min="0"
        max={duration || 1}
        step="1"
        value={shownPos}
        disabled={!duration}
        style:--fill="{duration ? (shownPos / duration) * 100 : 0}%"
        oninput={(e) => (dragging = +(e.currentTarget as HTMLInputElement).value)}
        onchange={(e) => { seekTo(+(e.currentTarget as HTMLInputElement).value); dragging = null; }}
        aria-label="Seek"
      />
      <span class="time">-{fmt(Math.max(0, duration - shownPos))}</span>
    </div>

    <div class="row">
      <button class="ib big" onclick={() => mpvPause()} aria-label={playing ? 'Pause' : 'Play'} data-tip={playing ? 'Pause · Space' : 'Play · Space'}>
        {#if playing}
          <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4.5" width="4.2" height="15" rx="1"/><rect x="13.8" y="4.5" width="4.2" height="15" rx="1"/></svg>
        {:else}
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 001.53.85l12-7.5a1 1 0 000-1.7l-12-7.5A1 1 0 007 4.5z"/></svg>
        {/if}
      </button>
      <button class="ib" onclick={() => seekBy(-10)} aria-label="Back 10 seconds" data-tip="Back 10s · ←">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12a8 8 0 108-8H8"/><polyline points="10.5 1.5 8 4 10.5 6.5"/><text x="12" y="15.5" font-size="7" font-weight="800" text-anchor="middle" fill="currentColor" stroke="none">10</text></svg>
      </button>
      <button class="ib" onclick={() => seekBy(10)} aria-label="Forward 10 seconds" data-tip="Forward 10s · →">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12a8 8 0 11-8-8h4"/><polyline points="13.5 1.5 16 4 13.5 6.5"/><text x="12" y="15.5" font-size="7" font-weight="800" text-anchor="middle" fill="currentColor" stroke="none">10</text></svg>
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
          value={$liveMuted ? 0 : $liveVolume}
          style:--fill="{(($liveMuted ? 0 : $liveVolume) / 130) * 100}%"
          oninput={(e) => setVolume(+(e.currentTarget as HTMLInputElement).value)}
          aria-label="Volume"
        />
      </div>

      <span class="grow center" dir="auto">
        {#if item}{item.title}{#if item.subtitle} <em>· {item.subtitle}</em>{/if}{/if}
      </span>

      {#if next}
        <button class="next" onclick={playNext} title="Next episode · N">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 6.3v11.4a.8.8 0 001.2.7L15 13v4.5h2V6.5h-2V11L7.2 5.6A.8.8 0 006 6.3z"/></svg>
          Next episode <small>{next.subtitle}</small>
        </button>
      {/if}
      <button class="ib" class:on={showTracks} onclick={() => (showTracks = !showTracks)} aria-label="Audio and subtitles" aria-expanded={showTracks} data-tip="Audio & subtitles · C">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="4.5" width="19" height="15" rx="2.5"/><path d="M7 11.5h4M13 11.5h4M7 15h7M16 15h1"/></svg>
      </button>
      <button class="ib" class:on={showHint} onclick={() => (showHint = !showHint)} aria-label="Keyboard shortcuts" data-tip="Shortcuts · ?">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6.5 10h1M10.5 10h1M14.5 10h1M8 14h8"/></svg>
      </button>
      <button class="ib" onclick={() => liveFullscreen.set(true)} aria-label="Fullscreen" data-tip="Fullscreen · F">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/></svg>
      </button>
    </div>

    {#if showTracks}
      <div transition:fade={{ duration: 120 }}><TrackPanel /></div>
    {/if}
    {#if showHint}
      <dl class="keys" transition:fade={{ duration: 120 }}>
        <div><dt>Space</dt><dd>Play / pause</dd></div>
        <div><dt>← →</dt><dd>Back / forward 10s</dd></div>
        <div><dt>↑ ↓</dt><dd>Volume</dd></div>
        <div><dt>M</dt><dd>Mute</dd></div>
        <div><dt>F</dt><dd>Fullscreen</dd></div>
        <div><dt>C</dt><dd>Audio & subtitles</dd></div>
        {#if isSeries}<div><dt>N</dt><dd>Next episode</dd></div>{/if}
        <div><dt>Esc</dt><dd>Back</dd></div>
      </dl>
    {/if}
    {#if $liveError}<p class="err">{$liveError}</p>{/if}
  </footer>
</div>

<style>
  .watch {
    height: 100vh;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    background: oklch(0.08 0.003 25);
    color: var(--color-text);
  }

  .top {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 20px;
  }
  .titles { min-width: 0; }
  .titles h1 {
    font-size: 1.125rem;
    font-weight: 800;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .titles p { font-size: 0.8125rem; color: var(--color-text-muted); font-weight: 600; }
  .grow { flex: 1; min-width: 0; }

  .screen {
    position: relative;
    display: grid;
    place-items: center;
    overflow: hidden;
    background: #000;
  }
  .screen-bg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(60px) brightness(0.35) saturate(1.3);
    transform: scale(1.2);
  }
  .screen-msg {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    text-align: center;
    color: var(--color-text-muted);
  }
  .screen-msg strong { font-size: 1.25rem; color: var(--color-text); }

  .controls {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 20px 14px;
    background: oklch(0.12 0.004 25);
    box-shadow: inset 0 1px 0 oklch(1 0 0 / 0.06);
  }

  .seek { display: flex; align-items: center; gap: 14px; }
  .time {
    min-width: 58px;
    font-size: 0.8125rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: oklch(0.85 0.004 25);
  }
  .time:last-child { text-align: end; }
  .seek input {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 5px;
    padding: 0;
    border: none;
    border-radius: 3px;
    background: linear-gradient(to right, var(--color-accent) var(--fill), oklch(1 0 0 / 0.2) var(--fill));
    cursor: pointer;
    transition: height 120ms var(--ease-out);
  }
  .seek input:hover { height: 7px; }
  .seek input::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 0 4px oklch(0.58 0.225 27 / 0.25);
  }
  .seek input:disabled { cursor: default; opacity: 0.5; }
  .seek input:focus-visible { outline: 2px solid var(--color-text); outline-offset: 6px; }

  .row { display: flex; align-items: center; gap: 6px; }
  .center {
    text-align: center;
    font-size: 0.9375rem;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 0 12px;
  }
  .center em { font-style: normal; color: var(--color-text-muted); font-weight: 600; }

  .ib {
    position: relative;
    flex-shrink: 0;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: none;
    color: oklch(0.9 0.004 25);
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .ib > :global(svg) { width: 24px; height: 24px; }
  .ib.big > :global(svg) { width: 28px; height: 28px; }
  .ib:hover { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  .ib:active { transform: scale(0.94); }
  .ib:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .ib.on { background: oklch(1 0 0 / 0.1); }
  /* Tooltips open upward, over the seek row (the video is further up) */
  .row .ib[data-tip]::after {
    content: attr(data-tip);
    position: absolute;
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translate(-50%, 4px);
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-size: 0.75rem;
    font-weight: 700;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
    transition: opacity 120ms var(--ease-out), transform 120ms var(--ease-out);
  }
  .row .ib:hover::after, .row .ib:focus-visible::after { opacity: 1; transform: translate(-50%, 0); }

  .vol { display: inline-flex; align-items: center; gap: 4px; margin-left: 4px; }
  .vol-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 96px;
    height: 4px;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: linear-gradient(to right, var(--color-text) var(--fill), oklch(1 0 0 / 0.2) var(--fill));
    cursor: pointer;
  }
  .vol-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--color-text);
  }

  .next {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 16px 0 12px;
    border-radius: 6px;
    background: oklch(1 0 0 / 0.1);
    color: var(--color-text);
    font-size: 0.875rem;
    font-weight: 700;
    transition: background 150ms var(--ease-out);
  }
  .next :global(svg) { width: 18px; height: 18px; }
  .next small { color: var(--color-text-muted); font-weight: 600; }
  .next:hover { background: oklch(1 0 0 / 0.18); }

  .upnext {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    margin-bottom: 4px;
    border-radius: 8px;
    background: oklch(0.58 0.225 27 / 0.16);
    box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.4);
    font-size: 0.9375rem;
  }
  .upnext span { flex: 1; }
  .btn {
    height: 36px;
    padding: 0 16px;
    border-radius: 6px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-size: 0.875rem;
    font-weight: 700;
  }
  .btn.ghost { background: oklch(1 0 0 / 0.1); color: var(--color-text); }

  .keys {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 8px 20px;
    padding: 10px 4px 2px;
    font-size: 0.8125rem;
    color: var(--color-text-muted);
  }
  .keys div { display: flex; align-items: center; gap: 9px; }
  .keys dt {
    padding: 2px 7px;
    border-radius: 4px;
    background: oklch(1 0 0 / 0.08);
    color: var(--color-text);
    font-size: 0.6875rem;
    font-weight: 700;
  }
  .err { font-size: 0.8125rem; color: var(--color-accent-soft); }

  .spinner {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 4px solid oklch(1 0 0 / 0.12);
    border-top-color: var(--color-accent);
    animation: spin 800ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (prefers-reduced-motion: reduce) {
    .spinner { animation: none; }
  }
</style>
