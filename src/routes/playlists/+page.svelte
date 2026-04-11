<script lang="ts">
  import { onMount } from 'svelte';
  import PlaylistItem from '$lib/components/PlaylistItem.svelte';
  import GroupList from '$lib/components/GroupList.svelte';
  import AddPlaylistModal from '$lib/components/AddPlaylistModal.svelte';
  import { deletePlaylist, mergePlaylists, splitPlaylist, exportPlaylist } from '$lib/tauri';
  import {
    playlists, selectedPlaylist, playlistGroups,
    loadPlaylists, loadPlaylistGroups,
  } from '$lib/stores/playlists';

  let showAddModal = $state(false);
  let actionLoading = $state(false);

  onMount(() => {
    loadPlaylists();
  });

  function selectPlaylist(p: typeof $selectedPlaylist) {
    selectedPlaylist.set(p);
    if (p) loadPlaylistGroups(p.id);
  }

  async function handleDelete() {
    const p = $selectedPlaylist;
    if (!p) return;
    actionLoading = true;
    try {
      await deletePlaylist(p.id);
      selectedPlaylist.set(null);
      playlistGroups.set([]);
      await loadPlaylists();
    } catch (e) {
      console.error('Failed to delete:', e);
    } finally {
      actionLoading = false;
    }
  }

  async function handleSplit() {
    const p = $selectedPlaylist;
    if (!p) return;
    actionLoading = true;
    try {
      await splitPlaylist(p.id);
      await loadPlaylists();
    } catch (e) {
      console.error('Failed to split:', e);
    } finally {
      actionLoading = false;
    }
  }

  async function handleExport() {
    const p = $selectedPlaylist;
    if (!p) return;
    // For now, export to a default path. In production, this would use a file dialog.
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
</script>

<div class="playlists-page fade-in">
  <div class="playlists-sidebar">
    <div class="sidebar-header">
      <h2>Playlists</h2>
      <button class="btn-accent add-btn" onclick={() => showAddModal = true}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
        Add
      </button>
    </div>
    <div class="playlist-list">
      {#each $playlists as p (p.id)}
        <PlaylistItem
          playlist={p}
          active={$selectedPlaylist?.id === p.id}
          onclick={() => selectPlaylist(p)}
        />
      {/each}
      {#if $playlists.length === 0}
        <div class="empty-state">
          <p>No playlists yet</p>
        </div>
      {/if}
    </div>
  </div>

  <div class="playlists-detail">
    {#if $selectedPlaylist}
      <div class="detail-header">
        <div>
          <h2>{$selectedPlaylist.name}</h2>
          <span class="detail-meta">Source: {$selectedPlaylist.source_type} &middot; {$selectedPlaylist.source_url}</span>
        </div>
        <div class="detail-actions">
          <button class="btn-ghost" onclick={handleExport} disabled={actionLoading}>Export</button>
          <button class="btn-ghost" onclick={handleSplit} disabled={actionLoading}>Split</button>
          <button class="btn-ghost danger" onclick={handleDelete} disabled={actionLoading}>Delete</button>
        </div>
      </div>
      <div class="detail-body">
        <GroupList groups={$playlistGroups} />
      </div>
    {:else}
      <div class="no-selection">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 6h16M4 10h16M4 14h10"/></svg>
        <p>Select a playlist to view details</p>
      </div>
    {/if}
  </div>
</div>

{#if showAddModal}
  <AddPlaylistModal onclose={() => showAddModal = false} />
{/if}

<style>
  .playlists-page {
    display: grid;
    grid-template-columns: 320px 1fr;
    gap: 1px;
    height: calc(100vh - 48px);
    margin: -24px;
    background: var(--color-border);
  }

  .playlists-sidebar {
    background: var(--color-base);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 16px 12px;
  }

  .sidebar-header h2 {
    font-size: 18px;
    font-weight: 700;
  }

  .add-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 12px;
    font-size: 13px;
  }

  .add-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  .playlist-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px 16px;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100px;
    color: var(--color-text-muted);
    font-size: 14px;
  }

  .playlists-detail {
    background: var(--color-base);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .detail-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 20px 24px 12px;
    border-bottom: 1px solid var(--color-border);
  }

  .detail-header h2 {
    font-size: 20px;
    font-weight: 700;
  }

  .detail-meta {
    font-size: 12px;
    color: var(--color-text-muted);
    margin-top: 4px;
    display: block;
  }

  .detail-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .detail-actions .danger {
    color: var(--color-accent);
  }

  .detail-actions .danger:hover {
    background: rgba(233, 69, 96, 0.1);
  }

  .detail-body {
    flex: 1;
    overflow-y: auto;
    padding: 16px 24px;
  }

  .no-selection {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-text-muted);
  }

  .no-selection :global(svg) {
    width: 48px;
    height: 48px;
    opacity: 0.3;
  }

  .no-selection p {
    font-size: 14px;
  }
</style>
