<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { embedPlace, mpvPause, mpvOsc } from '$lib/tauri';
  import {
    nowPlaying, liveStatus, liveFullscreen, videoAnchor, liveBehind, liveMuted, BEHIND_LIMIT,
    stopLive, goLive, notifySurfacePlaced, changeVolume, toggleMute, seekBy, overlayOpen,
  } from '$lib/stores/live';
  import { streamGuard } from '$lib/stores/streamGuard';

  // Keeps MPV's native in-app surface over the right spot: the Live TV screen,
  // the whole window in fullscreen, or a mini player in the corner elsewhere.

  const MINI_W = 384;
  const MINI_H = 216;
  const MARGIN = 20;
  const BAR_H = 56;

  let embedded = $derived(!!$nowPlaying?.embedded && $page.url.pathname !== '/player');
  // A page's video box only applies to its own kind (Live TV screen ≠ movie player)
  let anchorEl = $derived(
    $videoAnchor && $nowPlaying && $videoAnchor.kind === $nowPlaying.kind ? $videoAnchor.el : null,
  );
  let mini = $derived(embedded && !$liveFullscreen && !anchorEl);
  let viewport = $state({ w: 0, h: 0 });
  let behind = $derived($nowPlaying?.kind === 'live' && $liveStatus === 'live' && $liveBehind > BEHIND_LIMIT);

  let last = '';

  function target(): { x: number; y: number; w: number; h: number; visible: boolean } {
    if (!embedded || $overlayOpen) return { x: 0, y: 0, w: 0, h: 0, visible: false };
    if ($liveFullscreen) return { x: 0, y: 0, w: innerWidth, h: innerHeight, visible: true };
    const anchor = anchorEl;
    if (anchor && anchor.isConnected) {
      const r = anchor.getBoundingClientRect();
      return { x: r.left, y: r.top, w: r.width, h: r.height, visible: r.width > 0 && r.height > 0 };
    }
    return {
      x: innerWidth - MINI_W - MARGIN,
      y: innerHeight - MINI_H - BAR_H - MARGIN,
      w: MINI_W,
      h: MINI_H,
      visible: true,
    };
  }

  onMount(() => {
    let frame = 0;
    // Follow layout changes (resizes, transitions, route changes) every frame,
    // but only cross the IPC boundary when the rectangle actually moves.
    const loop = () => {
      viewport = { w: innerWidth, h: innerHeight };
      const t = target();
      const key = `${Math.round(t.x)},${Math.round(t.y)},${Math.round(t.w)},${Math.round(t.h)},${t.visible}`;
      if (key !== last) {
        last = key;
        embedPlace(t.x, t.y, t.w, t.h, t.visible)
          .then(() => t.visible && notifySurfacePlaced())
          .catch(() => {});
      }
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
  });

  // Real window fullscreen while the video fills it
  $effect(() => {
    const on = $liveFullscreen && embedded;
    try {
      getCurrentWindow().setFullscreen(on).catch(() => {});
    } catch {
      // Not running inside Tauri (e.g. the dev server in a browser)
    }
  });

  $effect(() => {
    if (!$nowPlaying) liveFullscreen.set(false);
  });

  // Movies: MPV's own seek bar while fullscreen (app controls can't sit on the video)
  $effect(() => {
    if ($nowPlaying?.kind === 'vod' && embedded) mpvOsc($liveFullscreen).catch(() => {});
  });

  function onKey(e: KeyboardEvent) {
    if (!$liveFullscreen) return;
    if (e.key === 'Escape' || e.key === 'f') {
      e.preventDefault();
      e.stopImmediatePropagation();
      liveFullscreen.set(false);
    } else if (e.key === ' ') {
      e.preventDefault();
      e.stopImmediatePropagation();
      mpvPause().catch(() => {});
    } else if ($nowPlaying?.kind === 'vod' && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
      e.preventDefault();
      e.stopImmediatePropagation();
      seekBy(e.key === 'ArrowLeft' ? -10 : 10);
    } else if (e.key === '+' || e.key === '=' || e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopImmediatePropagation();
      changeVolume(5);
    } else if (e.key === '-' || e.key === 'ArrowDown') {
      e.preventDefault();
      e.stopImmediatePropagation();
      changeVolume(-5);
    } else if (e.key === 'm') {
      e.stopImmediatePropagation();
      toggleMute();
    } else if (e.key === 'l') {
      e.stopImmediatePropagation();
      goLive();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if mini && $nowPlaying}
  <!-- The video itself is a native surface sitting right above this bar -->
  <div
    class="mini"
    style:left="{viewport.w - MINI_W - MARGIN}px"
    style:top="{viewport.h - BAR_H - MARGIN}px"
    style:width="{MINI_W}px"
    style:height="{BAR_H}px"
    transition:fade={{ duration: 150 }}
  >
    <span
      class="dot"
      class:live={$liveStatus === 'live' && !behind && $streamGuard.phase === 'ok'}
      class:behind={behind || $streamGuard.phase === 'failed' || $streamGuard.phase === 'blocked'}
      title={$streamGuard.phase === 'recovering' ? 'Reconnecting…' : undefined}
    ></span>
    <span class="name" dir="auto">{$nowPlaying.title}</span>
    {#if behind}
      <button class="golive" onclick={goLive} title="Jump to the live edge">Go live</button>
    {/if}
    <button onclick={toggleMute} aria-label={$liveMuted ? 'Unmute' : 'Mute'} title={$liveMuted ? 'Unmute' : 'Mute'}>
      {#if $liveMuted}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H3v6h3l5 4z" fill="currentColor"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/></svg>
      {:else}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H3v6h3l5 4z" fill="currentColor"/><path d="M15.5 8.5a5 5 0 010 7"/><path d="M18.5 5.5a9 9 0 010 13"/></svg>
      {/if}
    </button>
    <button onclick={() => mpvPause()} aria-label="Pause or resume" title="Pause">
      <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6.5" y="5" width="4" height="14" rx="1"/><rect x="13.5" y="5" width="4" height="14" rx="1"/></svg>
    </button>
    <button onclick={() => goto($nowPlaying!.href)} aria-label="Open player" title="Open player">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
    </button>
    <button onclick={() => stopLive()} aria-label="Stop" title="Stop">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
    </button>
  </div>
{/if}

<style>
  .mini {
    position: fixed;
    z-index: 300;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px 0 14px;
    border-radius: 0 0 10px 10px;
    background: oklch(0.2 0.005 25 / 0.97);
    box-shadow: 0 24px 50px -12px oklch(0 0 0 / 0.8), 0 0 0 1px oklch(1 0 0 / 0.08);
  }

  .dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-accent-yellow);
  }
  .dot.live { background: var(--color-accent-green); animation: pulse 1.6s ease-in-out infinite; }
  .dot.behind { background: var(--color-accent); }

  button.golive {
    width: auto;
    height: 30px;
    padding: 0 12px;
    border-radius: 5px;
    background: var(--color-accent);
    color: var(--color-on-accent);
    font-size: 0.75rem;
    font-weight: 800;
  }
  button.golive:hover { background: var(--color-accent-hover); }
  @keyframes pulse { 50% { opacity: 0.35; } }

  .name {
    flex: 1;
    min-width: 0;
    font-size: 0.875rem;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  button {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: none;
    color: var(--color-text);
    transition: background 150ms var(--ease-out);
  }
  button:hover { background: oklch(1 0 0 / 0.1); }
  button:focus-visible { outline: 2px solid var(--color-text); outline-offset: 1px; }
  button :global(svg) { width: 18px; height: 18px; }

  @media (prefers-reduced-motion: reduce) {
    .dot.live { animation: none; }
  }
</style>
