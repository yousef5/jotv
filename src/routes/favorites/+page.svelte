<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { favorites, loadFavorites } from '$lib/stores/favorites';
  import { currentChannel, isPlaying, viewStartTime } from '$lib/stores/player';
  import type { FavoriteChannel } from '$lib/tauri';

  onMount(() => {
    loadFavorites();
  });

  function playChannel(fav: FavoriteChannel) {
    currentChannel.set({
      id: fav.channel_id,
      playlist_id: 0,
      name: fav.channel_name,
      group_name: fav.group_name,
      stream_url: fav.stream_url,
      logo_url: fav.logo_url,
      epg_id: null,
      is_vod: false,
      created_at: '',
    });
    isPlaying.set(true);
    viewStartTime.set(Date.now());
    goto('/player');
  }
</script>

<div class="favorites-page fade-in">
  <h1 class="page-title">Favorites</h1>

  {#if $favorites.length === 0}
    <div class="empty">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
      <p>No favorites yet. Heart a channel to save it here.</p>
    </div>
  {:else}
    <div class="favorites-grid">
      {#each $favorites as fav (fav.id)}
        <button class="fav-card card" onclick={() => playChannel(fav)}>
          <div class="fav-thumb">
            {#if fav.logo_url}
              <img src={fav.logo_url} alt={fav.channel_name} loading="lazy" />
            {:else}
              <div class="fav-placeholder">
                <span>{fav.channel_name.charAt(0).toUpperCase()}</span>
              </div>
            {/if}
            <div class="play-overlay">
              <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            </div>
          </div>
          <div class="fav-info">
            <span class="fav-name">{fav.channel_name}</span>
            <span class="fav-meta">{fav.group_name} &middot; {fav.playlist_name}</span>
          </div>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .favorites-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding-bottom: 40px;
  }

  .page-title {
    font-size: 24px;
    font-weight: 700;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 80px 0;
    color: var(--color-text-muted);
  }

  .empty :global(svg) {
    width: 48px;
    height: 48px;
    opacity: 0.3;
  }

  .favorites-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 16px;
  }

  .fav-card {
    text-align: left;
    overflow: hidden;
    width: 100%;
  }

  .fav-thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 10;
    background: var(--color-surface);
    overflow: hidden;
  }

  .fav-thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    padding: 12px;
  }

  .fav-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--color-card), var(--color-surface));
  }

  .fav-placeholder span {
    font-size: 32px;
    font-weight: 700;
    color: var(--color-accent);
    opacity: 0.6;
  }

  .play-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    opacity: 0;
    transition: opacity var(--transition-fast);
  }

  .fav-card:hover .play-overlay {
    opacity: 1;
  }

  .play-overlay :global(svg) {
    width: 36px;
    height: 36px;
    color: #fff;
  }

  .fav-info {
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .fav-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .fav-meta {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
