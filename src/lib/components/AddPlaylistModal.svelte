<script lang="ts">
  import { onDestroy } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { addPlaylistFromUrl, addPlaylistFromFile, addPlaylistFromXtream } from '$lib/tauri';
  import { loadPlaylists } from '$lib/stores/playlists';

  let { onclose }: { onclose: () => void } = $props();

  let activeTab: 'url' | 'file' | 'xtream' = $state('url');
  let loading = $state(false);
  let error = $state('');
  let progressText = $state('');

  // URL tab
  let urlName = $state('');
  let urlValue = $state('');

  // File tab
  let fileName = $state('');
  let filePath = $state('');

  // Xtream tab
  let xtreamName = $state('');
  let xtreamServer = $state('');
  let xtreamUsername = $state('');
  let xtreamPassword = $state('');

  // Listen for xtream import progress events
  let unlisten: (() => void) | null = null;
  listen<{ stage: string; current: number; total: number }>('xtream-import-progress', (event) => {
    progressText = event.payload.stage;
  }).then((fn) => { unlisten = fn; });
  onDestroy(() => { unlisten?.(); });

  async function handleSubmit() {
    loading = true;
    error = '';
    progressText = '';
    try {
      if (activeTab === 'url') {
        if (!urlName || !urlValue) { error = 'Fill in all fields'; loading = false; return; }
        await addPlaylistFromUrl(urlName, urlValue);
      } else if (activeTab === 'file') {
        if (!fileName || !filePath) { error = 'Fill in all fields'; loading = false; return; }
        await addPlaylistFromFile(fileName, filePath);
      } else {
        if (!xtreamName || !xtreamServer || !xtreamUsername || !xtreamPassword) {
          error = 'Fill in all fields'; loading = false; return;
        }
        await addPlaylistFromXtream(xtreamName, xtreamServer, xtreamUsername, xtreamPassword);
      }
      await loadPlaylists();
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
      progressText = '';
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if ((e.target as HTMLElement).classList.contains('modal-backdrop')) {
      if (!loading) onclose();
    }
  }
</script>

<div class="modal-backdrop" role="button" tabindex="-1" onclick={handleBackdropClick} onkeydown={(e) => { if (e.key === 'Escape') onclose(); }}>
  <div class="modal fade-in">
    <div class="modal-header">
      <h2>Add Playlist</h2>
      <button class="close-btn" onclick={onclose} title="Close">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
      </button>
    </div>

    <div class="tab-bar">
      <button class="tab" class:active={activeTab === 'url'} onclick={() => activeTab = 'url'}>URL</button>
      <button class="tab" class:active={activeTab === 'file'} onclick={() => activeTab = 'file'}>File</button>
      <button class="tab" class:active={activeTab === 'xtream'} onclick={() => activeTab = 'xtream'}>Xtream</button>
    </div>

    <form class="modal-form" onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
      {#if activeTab === 'url'}
        <input type="text" placeholder="Playlist name" bind:value={urlName} />
        <input type="url" placeholder="M3U URL" bind:value={urlValue} />
      {:else if activeTab === 'file'}
        <input type="text" placeholder="Playlist name" bind:value={fileName} />
        <input type="text" placeholder="File path" bind:value={filePath} />
      {:else}
        <input type="text" placeholder="Playlist name" bind:value={xtreamName} />
        <input type="text" placeholder="Server URL" bind:value={xtreamServer} />
        <input type="text" placeholder="Username" bind:value={xtreamUsername} />
        <input type="password" placeholder="Password" bind:value={xtreamPassword} />
      {/if}

      {#if error}
        <div class="error">{error}</div>
      {/if}

      {#if loading && progressText}
        <div class="progress-info">{progressText}</div>
      {/if}

      <div class="modal-actions">
        <button type="button" class="btn-ghost" onclick={onclose} disabled={loading}>Cancel</button>
        <button type="submit" class="btn-accent" disabled={loading}>
          {loading ? 'Importing...' : 'Import'}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    backdrop-filter: blur(4px);
  }

  .modal {
    width: 100%;
    max-width: 440px;
    background: var(--color-sidebar);
    border: 1px solid var(--color-border);
    border-radius: 16px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .modal-header h2 {
    font-size: 18px;
    font-weight: 600;
  }

  .close-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text-muted);
  }

  .close-btn:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }

  .close-btn :global(svg) {
    width: 18px;
    height: 18px;
  }

  .tab-bar {
    display: flex;
    gap: 4px;
    background: var(--color-surface);
    border-radius: 10px;
    padding: 4px;
  }

  .tab {
    flex: 1;
    padding: 8px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-muted);
    background: transparent;
  }

  .tab:hover {
    color: var(--color-text);
  }

  .tab.active {
    color: var(--color-text);
    background: var(--color-card);
  }

  .modal-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .modal-form input {
    width: 100%;
  }

  .error {
    color: var(--color-accent);
    font-size: 13px;
    padding: 8px 12px;
    background: rgba(233, 69, 96, 0.1);
    border-radius: var(--radius-btn);
  }

  .progress-info {
    font-size: 12px;
    color: var(--color-text-muted);
    padding: 8px 12px;
    background: var(--color-surface);
    border-radius: var(--radius-btn);
    text-align: center;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.6; }
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
