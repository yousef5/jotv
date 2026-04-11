<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { playlists, loadPlaylists } from '$lib/stores/playlists';
  import { channels, loadChannels } from '$lib/stores/channels';
  import { currentChannel, isPlaying, viewStartTime } from '$lib/stores/player';
  import { getEpgForChannel, getCurrentProgram } from '$lib/tauri';
  import type { Channel, EpgEntry } from '$lib/tauri';

  let selectedPlaylistId: number | null = $state(null);
  let epgMap = $state<Map<number, { current: EpgEntry | null; next: EpgEntry | null }>>(new Map());
  let loadingEpg = $state(false);

  onMount(async () => {
    await loadPlaylists();
    // Auto-select first playlist
    if ($playlists.length > 0 && !selectedPlaylistId) {
      selectedPlaylistId = $playlists[0].id;
      await loadChannels($playlists[0].id);
      loadEpgData();
    }
  });

  async function handlePlaylistChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    selectedPlaylistId = Number(value);
    if (selectedPlaylistId) {
      await loadChannels(selectedPlaylistId);
      loadEpgData();
    }
  }

  async function loadEpgData() {
    loadingEpg = true;
    const newMap = new Map<number, { current: EpgEntry | null; next: EpgEntry | null }>();
    const channelsWithEpg = $channels.filter(c => c.epg_id);

    // Load in parallel, limited batches
    const batch = channelsWithEpg.slice(0, 50);
    await Promise.all(batch.map(async (ch) => {
      if (!ch.epg_id) return;
      try {
        const current = await getCurrentProgram(ch.epg_id);
        const allEpg = await getEpgForChannel(ch.epg_id);
        let next: EpgEntry | null = null;
        if (current && allEpg.length > 0) {
          const idx = allEpg.findIndex(e => e.id === current.id);
          if (idx >= 0 && idx + 1 < allEpg.length) {
            next = allEpg[idx + 1];
          }
        }
        newMap.set(ch.id, { current, next });
      } catch { /* ignore */ }
    }));

    epgMap = newMap;
    loadingEpg = false;
  }

  function playChannel(ch: Channel) {
    currentChannel.set(ch);
    isPlaying.set(true);
    viewStartTime.set(Date.now());
    goto('/player');
  }

  function formatTime(timeStr: string): string {
    try {
      return new Date(timeStr).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    } catch {
      return timeStr;
    }
  }
</script>

<div class="guide-page fade-in">
  <div class="page-header">
    <h1 class="page-title">TV Guide</h1>
    {#if $playlists.length > 0}
      <select class="playlist-select" onchange={handlePlaylistChange} value={selectedPlaylistId?.toString() ?? ''}>
        {#each $playlists as p (p.id)}
          <option value={p.id.toString()}>{p.name}</option>
        {/each}
      </select>
    {/if}
  </div>

  {#if loadingEpg}
    <div class="loading">Loading EPG data...</div>
  {/if}

  {#if $channels.length === 0}
    <div class="empty">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 10h18M9 4v16"/></svg>
      <p>Select a playlist to view the TV guide</p>
    </div>
  {:else}
    <div class="guide-list">
      {#each $channels as ch (ch.id)}
        {@const epg = epgMap.get(ch.id)}
        <button class="guide-row" onclick={() => playChannel(ch)}>
          <div class="guide-channel">
            {#if ch.logo_url}
              <img class="channel-logo" src={ch.logo_url} alt="" loading="lazy" />
            {:else}
              <div class="channel-initial">{ch.name.charAt(0)}</div>
            {/if}
            <span class="channel-name">{ch.name}</span>
          </div>
          <div class="guide-programs">
            {#if epg?.current}
              <div class="program current">
                <span class="program-time">{formatTime(epg.current.start_time)} - {formatTime(epg.current.end_time)}</span>
                <span class="program-title">{epg.current.title}</span>
              </div>
            {:else}
              <div class="program no-data">
                <span class="program-title">No EPG data</span>
              </div>
            {/if}
            {#if epg?.next}
              <div class="program next">
                <span class="program-time">{formatTime(epg.next.start_time)}</span>
                <span class="program-title">{epg.next.title}</span>
              </div>
            {/if}
          </div>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .guide-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding-bottom: 40px;
  }

  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .page-title {
    font-size: 24px;
    font-weight: 700;
  }

  .playlist-select {
    padding: 8px 12px;
    border-radius: var(--radius-btn);
    background: var(--color-card);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    font-size: 13px;
    min-width: 200px;
  }

  .loading {
    font-size: 13px;
    color: var(--color-text-muted);
    padding: 8px 0;
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

  .guide-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .guide-row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 14px;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text);
    text-align: left;
    width: 100%;
    transition: background var(--transition-fast);
  }

  .guide-row:hover {
    background: var(--color-card);
  }

  .guide-channel {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 220px;
    flex-shrink: 0;
  }

  .channel-logo {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    object-fit: contain;
    background: var(--color-surface);
  }

  .channel-initial {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    background: var(--color-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 700;
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .channel-name {
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .guide-programs {
    flex: 1;
    display: flex;
    gap: 16px;
    min-width: 0;
  }

  .program {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .program.current {
    flex: 2;
  }

  .program.next {
    flex: 1;
    opacity: 0.6;
  }

  .program.no-data {
    opacity: 0.4;
  }

  .program-time {
    font-size: 11px;
    color: var(--color-accent-green);
    font-weight: 500;
  }

  .program.next .program-time {
    color: var(--color-text-muted);
  }

  .program-title {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
