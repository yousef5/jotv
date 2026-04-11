<script lang="ts">
  import type { Playlist } from '$lib/tauri';

  let { playlist, active = false, onclick }: {
    playlist: Playlist;
    active?: boolean;
    onclick: () => void;
  } = $props();

  function formatDate(dateStr: string | null): string {
    if (!dateStr) return 'Never';
    try {
      return new Date(dateStr).toLocaleDateString();
    } catch {
      return dateStr;
    }
  }

  function sourceLabel(type: string): string {
    switch (type) {
      case 'm3u_url': return 'URL';
      case 'm3u_file': return 'File';
      case 'xtream': return 'Xtream';
      default: return type;
    }
  }
</script>

<button class="playlist-item" class:active onclick={onclick}>
  <div class="playlist-icon">
    {#if playlist.source_type === 'xtream'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><circle cx="6" cy="6" r="1"/><circle cx="6" cy="18" r="1"/></svg>
    {:else if playlist.source_type === 'm3u_url'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
    {:else}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>
    {/if}
  </div>
  <div class="playlist-info">
    <span class="playlist-name">{playlist.name}</span>
    <span class="playlist-meta">
      <span class="badge badge-purple">{sourceLabel(playlist.source_type)}</span>
      Updated: {formatDate(playlist.last_updated_at)}
    </span>
  </div>
</button>

<style>
  .playlist-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    text-align: left;
    padding: 12px;
    border-radius: 10px;
    background: transparent;
    color: var(--color-text);
    transition: all var(--transition-fast);
  }

  .playlist-item:hover {
    background: var(--color-hover);
  }

  .playlist-item.active {
    background: var(--color-card);
    border: 1px solid var(--color-border);
  }

  .playlist-icon {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: rgba(123, 104, 238, 0.12);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent-purple);
    flex-shrink: 0;
  }

  .playlist-icon :global(svg) {
    width: 18px;
    height: 18px;
  }

  .playlist-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .playlist-name {
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .playlist-meta {
    font-size: 11px;
    color: var(--color-text-muted);
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
