<script lang="ts" module>
  // Survives trips to the mini player and back
  let autoNext = true;
  let loop = false;
</script>

<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade } from 'svelte/transition';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { mpvPause, reelPlayed, reelUpdate, reelsDelete } from '$lib/tauri';
  import type { Reel } from '$lib/tauri';
  import {
    nowPlaying, liveStatus, liveError, liveFullscreen, videoAnchor, liveVolume, liveMuted, playbackPos,
    playMedia, stopLive, setVolume, changeVolume, toggleMute, seekBy, seekTo,
  } from '$lib/stores/live';
  import {
    reels, reelCats, reelsLoaded, reelQueue, loadReels, patchReel, thumbSrc, aspect, categoryPath, categoryColor,
    fmtDuration, reelChannel, fileSrc, PLATFORM_COLORS,
  } from '$lib/stores/reels';

  let id = $derived($page.url.searchParams.get('id') ?? '');
  let queue = $derived.by(() => {
    const ids = $reelQueue.length ? $reelQueue : [...$reels].sort((a, b) => b.created_at.localeCompare(a.created_at)).map((r) => r.id);
    const byId = new Map($reels.map((r) => [r.id, r]));
    return ids.map((i) => byId.get(i)).filter((r): r is Reel => !!r);
  });
  let index = $derived(queue.findIndex((r) => r.id === id));
  let reel = $derived(index >= 0 ? queue[index] : $reels.find((r) => r.id === id) ?? null);
  let prev = $derived(index > 0 ? queue[index - 1] : null);
  let next = $derived(index >= 0 && index + 1 < queue.length ? queue[index + 1] : null);

  let screenEl = $state<HTMLDivElement | undefined>();
  let listEl = $state<HTMLOListElement | undefined>();
  let now = $state(performance.now());
  let dragging = $state<number | null>(null);
  let auto = $state(autoNext);
  let looping = $state(loop);

  const SLIDE_SECONDS = 6;
  let isImage = $derived(reel?.kind === 'image');
  let slidePaused = $state(false);
  let slideStart = $state(performance.now());
  let slideFrac = $derived(isImage && auto && !slidePaused && next ? Math.min(1, (now - slideStart) / (SLIDE_SECONDS * 1000)) : 0);

  let mine = $derived(!!reel && $nowPlaying?.kind === 'vod' && $nowPlaying.url === reel.file_path);
  let playing = $derived(mine && $liveStatus === 'live');
  let starting = $derived(!mine || $liveStatus === 'starting' || $liveStatus === 'buffering');
  let duration = $derived(mine ? $playbackPos.duration : reel?.duration ?? 0);
  let position = $derived.by(() => {
    if (!mine) return 0;
    const p = $playbackPos;
    const ahead = playing ? (now - p.at) / 1000 : 0;
    return Math.min(p.duration || Infinity, p.pos + Math.max(0, ahead));
  });
  let shownPos = $derived(dragging ?? position);
  let path = $derived(reel ? categoryPath($reelCats, reel.category_id) : []);
  let bg = $derived(reel ? thumbSrc(reel) : null);

  $effect(() => {
    autoNext = auto;
    loop = looping;
  });

  $effect(() => {
    videoAnchor.set(screenEl && !isImage ? { el: screenEl, kind: 'vod' } : null);
    return () => videoAnchor.set(null);
  });

  // Photo slideshow
  $effect(() => {
    if (slideFrac >= 1) go(next);
  });

  // Start whatever the URL points at
  let started = '';
  $effect(() => {
    const r = reel;
    if (!r || started === r.id) return;
    started = r.id;
    start(r);
  });

  // End of a video: loop it, or move on
  let endedFor = '';
  $effect(() => {
    if (!mine || !$playbackPos.eof || !reel || endedFor === reel.id) return;
    endedFor = reel.id;
    if (looping) replay();
    else if (auto && next) go(next);
  });

  // Keep the current one visible in the queue
  $effect(() => {
    void index;
    tick().then(() => listEl?.querySelector('.on')?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));
  });

  onMount(() => {
    if (!$reelsLoaded) loadReels().catch(() => {});
    let frame = 0;
    const loopFrame = () => {
      now = performance.now();
      frame = requestAnimationFrame(loopFrame);
    };
    frame = requestAnimationFrame(loopFrame);
    return () => cancelAnimationFrame(frame);
  });

  async function start(r: Reel, fromStart = false) {
    endedFor = '';
    if (r.kind === 'image') {
      // A library video stops when a photo takes the screen (live TV keeps going)
      if ($nowPlaying?.channel.id === -1) stopLive();
      slideStart = performance.now();
      reelPlayed(r.id).catch(() => {});
      patchReel({ ...r, plays: r.plays + 1, last_played_at: new Date().toISOString().slice(0, 19).replace('T', ' ') });
      return;
    }
    if (mine && !fromStart) return; // already playing (back from the mini player)
    await playMedia(
      {
        channel: reelChannel(r),
        title: r.title,
        subtitle: path.map((c) => c.name).join(' › ') || r.platform,
        url: r.file_path,
        poster: thumbSrc(r),
        progressKey: `progress_reel_${r.id}`,
        href: `/reel?id=${encodeURIComponent(r.id)}`,
      },
      { fromStart },
    );
    reelPlayed(r.id).catch(() => {});
    patchReel({ ...r, plays: r.plays + 1, last_played_at: new Date().toISOString().slice(0, 19).replace('T', ' ') });
  }

  function replay() {
    if (!reel) return;
    endedFor = '';
    start(reel, true);
  }

  function go(r: Reel | null) {
    if (!r) return;
    goto(`/reel?id=${encodeURIComponent(r.id)}`, { replaceState: true, noScroll: true, keepFocus: true });
  }

  function back() {
    if ($liveFullscreen) return liveFullscreen.set(false);
    if (history.length > 1) history.back();
    else goto('/social');
  }

  async function close() {
    await stopLive();
    back();
  }

  async function toggleFav() {
    if (!reel) return;
    const r = reel;
    patchReel({ ...r, favorite: !r.favorite });
    try {
      patchReel(await reelUpdate(r.id, r.title === r.original_title ? null : r.title, r.category_id, r.tags, !r.favorite));
    } catch {
      patchReel(r);
    }
  }

  let askDelete = $state(false);

  async function deleteCurrent(files: boolean) {
    if (!reel) return;
    const gone = reel;
    const after = next ?? prev;
    askDelete = false;
    try {
      await reelsDelete([gone.id], files);
    } catch {
      return;
    }
    reels.update((l) => l.filter((r) => r.id !== gone.id));
    reelQueue.update((q) => q.filter((x) => x !== gone.id));
    if (after) go(after);
    else close();
  }

  // Wheel / swipe through the queue, one video per gesture
  let wheelLock = 0;
  function onWheel(e: WheelEvent) {
    if (Math.abs(e.deltaY) < 20 || performance.now() < wheelLock) return;
    wheelLock = performance.now() + 550;
    go(e.deltaY > 0 ? next : prev);
  }

  function onKey(e: KeyboardEvent) {
    if ($liveFullscreen) return; // VideoSurface handles keys in fullscreen
    if (e.target instanceof HTMLInputElement && e.target.type !== 'range') return;
    const k = e.key;
    if (k === ' ' || k === 'k') {
      e.preventDefault();
      if (isImage) { slidePaused = !slidePaused; slideStart = performance.now(); }
      else mpvPause().catch(() => {});
    }
    else if (k === 'ArrowDown' || k === 'PageDown' || k === 'n') { e.preventDefault(); go(next); }
    else if (k === 'ArrowUp' || k === 'PageUp' || k === 'p') { e.preventDefault(); go(prev); }
    else if (k === 'ArrowLeft' || k === 'j') { e.preventDefault(); seekBy(-5); }
    else if (k === 'ArrowRight' || k === 'l') { e.preventDefault(); seekBy(5); }
    else if (k === '+' || k === '=') changeVolume(5);
    else if (k === '-') changeVolume(-5);
    else if (k === 'm') toggleMute();
    else if (k === 'f') liveFullscreen.set(true);
    else if (k === 'r') looping = !looping;
    else if (k === 'h') toggleFav();
    else if (k === 'Delete') askDelete = true;
    else if (k === 'Escape') askDelete ? (askDelete = false) : back();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="reel-page">
  {#if bg}
    {#key bg}<img class="ambient" src={bg} alt="" aria-hidden="true" in:fade={{ duration: 500 }} />{/key}
  {/if}

  <header class="top">
    <button class="ib" onclick={back} aria-label="Back" title="Back · Esc">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
    </button>
    {#if path.length}
      <p class="crumb">
        <span class="dot" style:background={categoryColor($reelCats, reel?.category_id ?? null)}></span>
        {path.map((c) => c.name).join(' › ')}
      </p>
    {/if}
    <span class="grow"></span>
    {#if index >= 0}<span class="counter">{index + 1} / {queue.length}</span>{/if}
    <button class="ib" onclick={close} aria-label="Stop and close" title="Stop and close">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
    </button>
  </header>

  <div class="body">
    <!-- Stage: the video, with prev/next beside it -->
    <section class="stage" onwheel={onWheel} aria-label="Player">
      <div class="fit" style:--a={reel ? aspect(reel) : 16 / 9}>
        <!-- MPV's native surface is placed exactly over this box -->
        <div class="screen" bind:this={screenEl}>
          {#if !reel && $reelsLoaded}
            <div class="msg"><strong>Video not found</strong><span>It may have been removed from the library.</span><button class="btn" onclick={back}>Go back</button></div>
          {:else if isImage && reel}
            {#key reel.id}<img class="photo" src={fileSrc(reel)} alt={reel.title} in:fade={{ duration: 220 }} />{/key}
          {:else if starting || !$nowPlaying?.embedded}
            <div class="msg">
              {#if mine && !$nowPlaying?.embedded}
                <strong>Playing in a separate MPV window</strong>
              {:else}
                {#if bg}<img class="poster" src={bg} alt="" />{/if}
                <span class="spinner" aria-label="Loading"></span>
              {/if}
            </div>
          {/if}
        </div>
      </div>

      <div class="nav">
        <button class="navbtn" onclick={() => go(prev)} disabled={!prev} aria-label="Previous video" title="Previous · ↑">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 15 12 9 18 15"/></svg>
        </button>
        <button class="navbtn" onclick={() => go(next)} disabled={!next} aria-label="Next video" title="Next · ↓">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
        </button>
      </div>
    </section>

    <!-- Details + queue -->
    <aside class="side">
      {#if reel}
        <div class="info">
          <h1 dir="auto">{reel.title}</h1>
          <p class="facts">
            <span class="plat" style:--c={PLATFORM_COLORS[reel.platform] ?? 'var(--color-text-muted)'}>{reel.platform}</span>
            {#if reel.duration}<span>{fmtDuration(reel.duration)}</span>{/if}
            {#if reel.plays}<span>{reel.plays} play{reel.plays === 1 ? '' : 's'}</span>{/if}
          </p>
          {#if reel.tags.length}
            <p class="tags">{#each reel.tags as t (t)}<span>#{t}</span>{/each}</p>
          {/if}
          <div class="actions">
            <button class="pill" class:on={reel.favorite} onclick={toggleFav} title="Favorite · H">
              <svg viewBox="0 0 24 24" fill={reel.favorite ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M12 20.5s-7.5-4.6-9.2-9.4C1.6 7.6 4 4.5 7.2 4.5c2 0 3.6 1.1 4.8 2.8 1.2-1.7 2.8-2.8 4.8-2.8 3.2 0 5.6 3.1 4.4 6.6-1.7 4.8-9.2 9.4-9.2 9.4z"/></svg>
              Favorite
            </button>
            <button class="pill" class:on={looping} onclick={() => (looping = !looping)} aria-pressed={looping} title="Repeat this video · R">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="17 2 21 6 17 10"/><path d="M3 11V9a3 3 0 013-3h15"/><polyline points="7 22 3 18 7 14"/><path d="M21 13v2a3 3 0 01-3 3H3"/></svg>
              Repeat
            </button>
            <button class="pill" class:on={auto} onclick={() => (auto = !auto)} aria-pressed={auto} title="Play the next one automatically">
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 6.3v11.4a.8.8 0 001.2.7L15 13v4.5h2V6.5h-2V11L7.2 5.6A.8.8 0 006 6.3z"/></svg>
              Autoplay
            </button>
            <button class="pill del" onclick={() => (askDelete = true)} title="Delete · Del" aria-label="Delete video">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16"/><path d="M9.5 7V4.8a.8.8 0 01.8-.8h3.4a.8.8 0 01.8.8V7"/><path d="M6.5 7l.8 12.2A1.8 1.8 0 009.1 21h5.8a1.8 1.8 0 001.8-1.8L17.5 7"/><path d="M10 11v6M14 11v6"/></svg>
            </button>
          </div>
          {#if askDelete}
            <div class="ask" transition:fade={{ duration: 120 }} role="alertdialog" aria-label="Delete this video?">
              <b>Delete this video?</b>
              <button class="abtn danger" onclick={() => deleteCurrent(true)}>Delete file from disk</button>
              <button class="abtn" onclick={() => deleteCurrent(false)}>Only remove from library</button>
              <button class="abtn plain" onclick={() => (askDelete = false)}>Cancel</button>
            </div>
          {/if}
        </div>
      {/if}

      {#if queue.length > 1}
        <p class="qhead">Up next <em>{Math.max(0, queue.length - index - 1)}</em></p>
        <ol class="queue" bind:this={listEl}>
          {#each queue as q, i (q.id)}
            {@const src = thumbSrc(q)}
            <li>
              <button class="qi" class:on={q.id === id} class:past={i < index} onclick={() => go(q)}>
                <span class="qthumb" style:--a={aspect(q)}>
                  {#if src}<img src={src} alt="" loading="lazy" />{/if}
                  {#if q.id === id && playing}
                    <span class="eq" aria-label="Playing"><i></i><i></i><i></i></span>
                  {/if}
                </span>
                <span class="qtext">
                  <b dir="auto">{q.title}</b>
                  <small>{q.platform}{q.duration ? ` · ${fmtDuration(q.duration)}` : ''}</small>
                </span>
              </button>
            </li>
          {/each}
        </ol>
      {/if}
    </aside>
  </div>

  <footer class="controls">
    {#if isImage}
      <div class="slide" aria-hidden="true"><i style:width="{slideFrac * 100}%"></i></div>
    {/if}
    <div class="seek" hidden={isImage}>
      <span class="time">{fmtDuration(shownPos) || '0:00'}</span>
      <input
        type="range"
        min="0"
        max={duration || 1}
        step="0.1"
        value={shownPos}
        disabled={!mine || !duration}
        style:--fill="{duration ? (shownPos / duration) * 100 : 0}%"
        oninput={(e) => (dragging = +(e.currentTarget as HTMLInputElement).value)}
        onchange={(e) => { seekTo(+(e.currentTarget as HTMLInputElement).value); dragging = null; }}
        aria-label="Seek"
      />
      <span class="time end">{fmtDuration(duration) || '0:00'}</span>
    </div>
    <div class="row">
      <button class="ib" onclick={() => go(prev)} disabled={!prev} aria-label="Previous" title="Previous · ↑">
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M18 6.3v11.4a.8.8 0 01-1.2.7L9 13v4.5H7V6.5h2V11l7.8-5.4a.8.8 0 011.2.7z"/></svg>
      </button>
      <button
        class="ib big"
        onclick={() => (isImage ? ((slidePaused = !slidePaused), (slideStart = performance.now())) : mpvPause())}
        aria-label={(isImage ? !slidePaused && auto : playing) ? 'Pause' : 'Play'}
        title={isImage ? 'Slideshow · Space' : 'Play / pause · Space'}
      >
        {#if isImage ? !slidePaused && auto : playing}
          <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4.5" width="4.2" height="15" rx="1"/><rect x="13.8" y="4.5" width="4.2" height="15" rx="1"/></svg>
        {:else}
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 001.53.85l12-7.5a1 1 0 000-1.7l-12-7.5A1 1 0 007 4.5z"/></svg>
        {/if}
      </button>
      <button class="ib" onclick={() => go(next)} disabled={!next} aria-label="Next" title="Next · ↓">
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 6.3v11.4a.8.8 0 001.2.7L15 13v4.5h2V6.5h-2V11L7.2 5.6A.8.8 0 006 6.3z"/></svg>
      </button>
      <div class="vol" hidden={isImage}>
        <button class="ib" onclick={toggleMute} aria-label={$liveMuted ? 'Unmute' : 'Mute'} title="Mute · M">
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
      <span class="grow"></span>
      <span class="keys">↑ ↓ switch · ← → 5s · Space pause</span>
      <button class="ib" onclick={() => liveFullscreen.set(true)} aria-label="Fullscreen" title="Fullscreen · F">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/></svg>
      </button>
    </div>
    {#if $liveError}<p class="err">{$liveError}</p>{/if}
  </footer>
</div>

<style>
  [dir="auto"] { text-align: left; }

  .reel-page {
    position: relative;
    height: 100vh;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    background: oklch(0.09 0.003 25);
    color: var(--color-text);
    overflow: hidden;
    isolation: isolate;
  }
  /* Ambient light from the current video, like a TV bias light */
  .ambient {
    position: absolute;
    inset: -80px;
    width: calc(100% + 160px);
    height: calc(100% + 160px);
    object-fit: cover;
    filter: blur(80px) saturate(1.5) brightness(0.32);
    z-index: -1;
    pointer-events: none;
  }

  .top { display: flex; align-items: center; gap: 14px; padding: 10px 18px; }
  .crumb { display: flex; align-items: center; gap: 8px; font-size: 0.875rem; font-weight: 700; color: oklch(0.85 0.004 25); }
  .dot { width: 9px; height: 9px; border-radius: 50%; }
  .counter { font-size: 0.8125rem; font-weight: 700; font-variant-numeric: tabular-nums; color: var(--color-text-muted); }
  .grow { flex: 1; min-width: 0; }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 360px;
    gap: 20px;
    padding: 0 18px 0 24px;
    min-height: 0;
  }

  .stage { position: relative; display: flex; align-items: center; gap: 18px; min-height: 0; min-width: 0; }
  .fit {
    flex: 1;
    align-self: stretch;
    display: grid;
    place-items: center;
    container-type: size;
    min-width: 0;
  }
  .screen {
    position: relative;
    width: min(100cqw, 100cqh * var(--a));
    height: min(100cqh, 100cqw / var(--a));
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 12px;
    background: #000;
    box-shadow: 0 40px 80px -30px oklch(0 0 0 / 0.9);
  }
  .msg {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
    color: var(--color-text-muted);
    padding: 16px;
  }
  .msg strong { font-size: 1.125rem; color: var(--color-text); }
  .photo { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; background: #000; }
  .slide { height: 3px; border-radius: 2px; background: oklch(1 0 0 / 0.12); overflow: hidden; margin-bottom: 6px; }
  .slide i { display: block; height: 100%; background: var(--color-text); }
  [hidden] { display: none !important; }
  .poster { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: 0.45; }
  .spinner {
    position: relative;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 4px solid oklch(1 0 0 / 0.15);
    border-top-color: var(--color-text);
    animation: spin 800ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .nav { display: flex; flex-direction: column; gap: 12px; }
  .navbtn {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(1 0 0 / 0.1);
    color: var(--color-text);
    transition: background 150ms var(--ease-out), transform 150ms var(--ease-out), opacity 150ms;
  }
  .navbtn :global(svg) { width: 24px; height: 24px; }
  .navbtn:hover { background: oklch(1 0 0 / 0.2); transform: scale(1.06); }
  .navbtn:disabled { opacity: 0.25; pointer-events: none; }
  .navbtn:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  /* ── Side ── */
  .side { display: flex; flex-direction: column; min-height: 0; gap: 14px; }
  .info {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border-radius: 12px;
    background: oklch(0.16 0.004 25 / 0.8);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.07);
  }
  .info h1 {
    font-size: 1.125rem;
    font-weight: 800;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .facts { display: flex; flex-wrap: wrap; gap: 4px 12px; font-size: 0.8125rem; font-weight: 600; color: var(--color-text-muted); }
  .plat { display: inline-flex; align-items: center; gap: 6px; color: var(--color-text); font-weight: 700; }
  .plat::before { content: ''; width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
  .tags { display: flex; flex-wrap: wrap; gap: 6px; }
  .tags span { padding: 3px 9px; border-radius: 12px; background: oklch(1 0 0 / 0.08); font-size: 0.75rem; font-weight: 600; color: oklch(0.88 0.004 25); }
  .actions { display: flex; flex-wrap: wrap; gap: 6px; }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 34px;
    padding: 0 12px;
    border-radius: 17px;
    background: oklch(1 0 0 / 0.08);
    color: oklch(0.88 0.004 25);
    font-size: 0.8125rem;
    font-weight: 700;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .pill :global(svg) { width: 16px; height: 16px; }
  .pill:hover { background: oklch(1 0 0 / 0.14); color: var(--color-text); }
  .pill.on { background: var(--color-text); color: oklch(0.16 0.004 25); }

  .pill.del { margin-left: auto; padding: 0 10px; }
  .pill.del:hover { background: var(--color-accent); color: var(--color-on-accent); }
  .ask {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border-radius: 10px;
    background: oklch(0.58 0.225 27 / 0.12);
    box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.5);
  }
  .ask b { font-size: 0.875rem; margin-bottom: 2px; }
  .abtn { height: 34px; border-radius: 7px; background: oklch(1 0 0 / 0.1); color: var(--color-text); font-size: 0.8125rem; font-weight: 700; }
  .abtn:hover { background: oklch(1 0 0 / 0.16); }
  .abtn.danger { background: var(--color-accent); color: var(--color-on-accent); }
  .abtn.danger:hover { background: var(--color-accent-hover); }
  .abtn.plain { background: none; color: var(--color-text-muted); }
  .abtn.plain:hover { color: var(--color-text); }

  .qhead { display: flex; gap: 8px; align-items: baseline; padding: 0 4px; font-size: 0.6875rem; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: var(--color-text-muted); }
  .qhead em { font-style: normal; letter-spacing: 0; }
  .queue { list-style: none; margin: 0; padding: 0 4px 8px 0; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; scrollbar-width: thin; min-height: 0; }
  .qi {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 6px;
    border-radius: 9px;
    background: none;
    text-align: start;
    color: var(--color-text);
    transition: background 120ms var(--ease-out);
  }
  .qi:hover { background: oklch(1 0 0 / 0.07); }
  .qi.on { background: oklch(1 0 0 / 0.12); }
  .qi.past { opacity: 0.55; }
  .qi:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .qthumb {
    position: relative;
    flex-shrink: 0;
    width: 96px;
    height: 60px;
    border-radius: 6px;
    overflow: hidden;
    background: oklch(0.2 0.005 25);
  }
  .qthumb img { width: 100%; height: 100%; object-fit: cover; }
  .qtext { min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .qtext b { font-size: 0.8125rem; font-weight: 700; line-height: 1.3; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .qtext small { font-size: 0.6875rem; font-weight: 600; color: var(--color-text-muted); }
  .eq { position: absolute; inset: 0; display: flex; align-items: flex-end; justify-content: center; gap: 3px; padding-bottom: 14px; background: oklch(0 0 0 / 0.5); }
  .eq i { width: 4px; height: 16px; border-radius: 2px; background: var(--color-text); transform-origin: bottom; animation: eq 900ms ease-in-out infinite; }
  .eq i:nth-child(2) { animation-delay: -300ms; }
  .eq i:nth-child(3) { animation-delay: -600ms; }
  @keyframes eq { 0%, 100% { transform: scaleY(0.3); } 50% { transform: scaleY(1); } }

  /* ── Controls ── */
  .controls { display: flex; flex-direction: column; gap: 4px; padding: 10px 24px 14px; }
  .seek { display: flex; align-items: center; gap: 14px; }
  .time { min-width: 46px; font-size: 0.8125rem; font-weight: 700; font-variant-numeric: tabular-nums; color: oklch(0.85 0.004 25); }
  .time.end { text-align: end; }
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
  }
  .seek input::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 0 4px oklch(0.58 0.225 27 / 0.25);
  }
  .seek input:disabled { opacity: 0.5; cursor: default; }
  .row { display: flex; align-items: center; gap: 4px; }
  .ib {
    flex-shrink: 0;
    width: 42px;
    height: 42px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: none;
    color: oklch(0.9 0.004 25);
    transition: background 120ms var(--ease-out);
  }
  .ib :global(svg) { width: 22px; height: 22px; }
  .ib.big :global(svg) { width: 28px; height: 28px; }
  .ib:hover { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  .ib:disabled { opacity: 0.3; pointer-events: none; }
  .ib:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .vol { display: inline-flex; align-items: center; gap: 4px; margin-left: 6px; }
  .vol-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 92px;
    height: 4px;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: linear-gradient(to right, var(--color-text) var(--fill), oklch(1 0 0 / 0.2) var(--fill));
    cursor: pointer;
  }
  .vol-slider::-webkit-slider-thumb { -webkit-appearance: none; width: 13px; height: 13px; border-radius: 50%; background: var(--color-text); }
  .keys { font-size: 0.75rem; font-weight: 600; color: var(--color-text-muted); margin-right: 8px; }
  .btn { height: 36px; padding: 0 16px; border-radius: 6px; background: var(--color-text); color: oklch(0.14 0.004 25); font-size: 0.875rem; font-weight: 700; }
  .err { font-size: 0.8125rem; color: var(--color-accent-soft); }

  @media (max-width: 1100px) {
    .body { grid-template-columns: minmax(0, 1fr) 290px; }
    .keys { display: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner, .eq i { animation: none; }
    .navbtn:hover { transform: none; }
  }
</style>
