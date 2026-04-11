<script lang="ts">
  import { addPlaylistFromUrl, addPlaylistFromFile, addPlaylistFromXtream } from '$lib/tauri';
  import { loadPlaylists } from '$lib/stores/playlists';

  let activeTab: 'url' | 'file' | 'xtream' = $state('url');
  let loading = $state(false);
  let error = $state('');

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

  async function handleSubmit() {
    loading = true;
    error = '';
    try {
      if (activeTab === 'url') {
        if (!urlName || !urlValue) { error = 'Please fill in all fields'; loading = false; return; }
        await addPlaylistFromUrl(urlName, urlValue);
      } else if (activeTab === 'file') {
        if (!fileName || !filePath) { error = 'Please fill in all fields'; loading = false; return; }
        await addPlaylistFromFile(fileName, filePath);
      } else {
        if (!xtreamName || !xtreamServer || !xtreamUsername || !xtreamPassword) {
          error = 'Please fill in all fields'; loading = false; return;
        }
        await addPlaylistFromXtream(xtreamName, xtreamServer, xtreamUsername, xtreamPassword);
      }
      await loadPlaylists();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="welcome">
  <div class="welcome-content">
    <div class="welcome-header">
      <div class="welcome-logo">J</div>
      <h1>Welcome to JoTV</h1>
      <p>Your personal IPTV player. Get started by adding a playlist.</p>
    </div>

    <div class="tab-bar">
      <button class="tab" class:active={activeTab === 'url'} onclick={() => activeTab = 'url'}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
        URL
      </button>
      <button class="tab" class:active={activeTab === 'file'} onclick={() => activeTab = 'file'}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>
        File
      </button>
      <button class="tab" class:active={activeTab === 'xtream'} onclick={() => activeTab = 'xtream'}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><circle cx="6" cy="6" r="1"/><circle cx="6" cy="18" r="1"/></svg>
        Xtream
      </button>
    </div>

    <form class="import-form" onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
      {#if activeTab === 'url'}
        <input type="text" placeholder="Playlist name" bind:value={urlName} />
        <input type="url" placeholder="M3U URL (http://...)" bind:value={urlValue} />
      {:else if activeTab === 'file'}
        <input type="text" placeholder="Playlist name" bind:value={fileName} />
        <input type="text" placeholder="File path (/path/to/playlist.m3u)" bind:value={filePath} />
      {:else}
        <input type="text" placeholder="Playlist name" bind:value={xtreamName} />
        <input type="text" placeholder="Server URL" bind:value={xtreamServer} />
        <input type="text" placeholder="Username" bind:value={xtreamUsername} />
        <input type="password" placeholder="Password" bind:value={xtreamPassword} />
      {/if}

      {#if error}
        <div class="error">{error}</div>
      {/if}

      <button type="submit" class="btn-accent submit-btn" disabled={loading}>
        {loading ? 'Importing...' : 'Import Playlist'}
      </button>
    </form>
  </div>
</div>

<style>
  .welcome {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: calc(100vh - 48px);
  }

  .welcome-content {
    width: 100%;
    max-width: 460px;
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  .welcome-header {
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .welcome-logo {
    width: 64px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 32px;
    font-weight: 800;
    color: var(--color-accent);
    background: rgba(233, 69, 96, 0.12);
    border-radius: 16px;
    margin-bottom: 8px;
  }

  .welcome-header h1 {
    font-size: 28px;
    font-weight: 700;
    color: var(--color-text);
  }

  .welcome-header p {
    font-size: 14px;
    color: var(--color-text-muted);
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
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 10px;
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
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }

  .tab :global(svg) {
    width: 16px;
    height: 16px;
  }

  .import-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .import-form input {
    width: 100%;
  }

  .error {
    color: var(--color-accent);
    font-size: 13px;
    padding: 8px 12px;
    background: rgba(233, 69, 96, 0.1);
    border-radius: var(--radius-btn);
  }

  .submit-btn {
    margin-top: 4px;
    padding: 12px;
    font-size: 15px;
    font-weight: 600;
  }
</style>
