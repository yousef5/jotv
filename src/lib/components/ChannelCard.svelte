<script lang="ts">
  import { goto } from '$app/navigation';
  import type { Channel } from '$lib/tauri';
  import { currentChannel, isPlaying, viewStartTime } from '$lib/stores/player';

  let { channel }: { channel: Channel } = $props();

  function playChannel() {
    currentChannel.set(channel);
    isPlaying.set(true);
    viewStartTime.set(Date.now());
    goto('/player');
  }
</script>

<button class="channel-card" onclick={playChannel}>
  <div class="channel-thumb">
    {#if channel.logo_url}
      <img src={channel.logo_url} alt={channel.name} loading="lazy" />
    {:else}
      <div class="channel-placeholder">
        <span>{channel.name.charAt(0).toUpperCase()}</span>
      </div>
    {/if}
    <div class="play-overlay">
      <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
    </div>
  </div>
  <div class="channel-info">
    <span class="channel-name" title={channel.name}>{channel.name}</span>
    <span class="channel-group">{channel.group_name || 'Uncategorized'}</span>
  </div>
</button>

<style>
  .channel-card {
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-card);
    overflow: hidden;
    text-align: left;
    width: 100%;
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .channel-card:hover {
    transform: translateY(-3px);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4);
  }

  .channel-thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 10;
    background: var(--color-surface);
    overflow: hidden;
  }

  .channel-thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    padding: 12px;
  }

  .channel-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--color-card), var(--color-surface));
  }

  .channel-placeholder span {
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

  .channel-card:hover .play-overlay {
    opacity: 1;
  }

  .play-overlay :global(svg) {
    width: 40px;
    height: 40px;
    color: #fff;
    filter: drop-shadow(0 2px 4px rgba(0,0,0,0.3));
  }

  .channel-info {
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .channel-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .channel-group {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
