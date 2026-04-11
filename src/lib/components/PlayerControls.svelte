<script lang="ts">
  import { goto } from '$app/navigation';
  import type { Channel, EpgEntry } from '$lib/tauri';
  import { toggleFavorite, isFavorite, queueDownload } from '$lib/tauri';

  let {
    channel,
    currentProgram = null,
    videoEl = undefined,
  }: {
    channel: Channel;
    currentProgram?: EpgEntry | null;
    videoEl?: HTMLVideoElement;
  } = $props();

  let visible = $state(true);
  let hideTimeout: ReturnType<typeof setTimeout> | null = null;
  let isFav = $state(false);
  let paused = $state(false);

  async function checkFavorite() {
    try {
      isFav = await isFavorite(channel.id);
    } catch { /* ignore */ }
  }

  $effect(() => {
    if (channel) {
      checkFavorite();
    }
  });

  function showControls() {
    visible = true;
    resetHideTimer();
  }

  function resetHideTimer() {
    if (hideTimeout) clearTimeout(hideTimeout);
    hideTimeout = setTimeout(() => {
      visible = false;
    }, 3000);
  }

  function handleMouseMove() {
    showControls();
  }

  async function handleToggleFavorite() {
    try {
      isFav = await toggleFavorite(channel.id);
    } catch (e) {
      console.error('Failed to toggle favorite:', e);
    }
  }

  function handlePlayPause() {
    if (!videoEl) return;
    if (videoEl.paused) {
      videoEl.play();
      paused = false;
    } else {
      videoEl.pause();
      paused = true;
    }
  }

  async function handleDownload() {
    try {
      await queueDownload(channel.id, channel.stream_url);
    } catch (e) {
      console.error('Failed to queue download:', e);
    }
  }

  function handleFullscreen() {
    if (document.fullscreenElement) {
      document.exitFullscreen();
    } else {
      document.documentElement.requestFullscreen();
    }
  }

  function handleBack() {
    goto('/');
  }

  // Start the hide timer on mount
  $effect(() => {
    resetHideTimer();
    return () => {
      if (hideTimeout) clearTimeout(hideTimeout);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="controls-wrapper" onmousemove={handleMouseMove} class:hidden={!visible}>
  <!-- Top bar -->
  <div class="controls-top glass-heavy">
    <button class="ctrl-btn" onclick={handleBack} title="Back">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M19 12H5M12 19l-7-7 7-7"/></svg>
    </button>
    <div class="channel-title">
      <span class="channel-name">{channel.name}</span>
      {#if channel.group_name}
        <span class="channel-group">{channel.group_name}</span>
      {/if}
    </div>
    <div class="top-actions">
      <button class="ctrl-btn" class:active-fav={isFav} onclick={handleToggleFavorite} title="Favorite">
        <svg viewBox="0 0 24 24" fill={isFav ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
      </button>
      <button class="ctrl-btn" onclick={handleDownload} title="Download">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
      </button>
      <button class="ctrl-btn" onclick={handleFullscreen} title="Fullscreen">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
      </button>
    </div>
  </div>

  <!-- Center play/pause -->
  <button class="center-play" onclick={handlePlayPause}>
    {#if paused}
      <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
    {:else}
      <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
    {/if}
  </button>

  <!-- Bottom bar -->
  <div class="controls-bottom glass-heavy">
    {#if currentProgram}
      <div class="epg-info">
        <span class="epg-now">Now: {currentProgram.title}</span>
        {#if currentProgram.description}
          <span class="epg-desc">{currentProgram.description}</span>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .controls-wrapper {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    z-index: 10;
    transition: opacity 0.3s ease;
  }

  .controls-wrapper.hidden {
    opacity: 0;
    pointer-events: none;
  }

  .controls-top {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
  }

  .channel-title {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .channel-name {
    font-size: 16px;
    font-weight: 600;
    color: #fff;
  }

  .channel-group {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
  }

  .top-actions {
    display: flex;
    gap: 4px;
  }

  .ctrl-btn {
    width: 38px;
    height: 38px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: transparent;
    color: rgba(255, 255, 255, 0.8);
  }

  .ctrl-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .ctrl-btn.active-fav {
    color: var(--color-accent);
  }

  .ctrl-btn :global(svg) {
    width: 20px;
    height: 20px;
  }

  .center-play {
    align-self: center;
    width: 64px;
    height: 64px;
    border-radius: 50%;
    background: rgba(233, 69, 96, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    backdrop-filter: blur(8px);
  }

  .center-play:hover {
    background: rgba(233, 69, 96, 1);
    transform: scale(1.08);
  }

  .center-play :global(svg) {
    width: 28px;
    height: 28px;
  }

  .controls-bottom {
    padding: 12px 16px;
  }

  .epg-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .epg-now {
    font-size: 14px;
    font-weight: 500;
    color: #fff;
  }

  .epg-desc {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
