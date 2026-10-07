<script lang="ts">
  import { onMount } from 'svelte';
  import {
    theme, externalPlayer, externalPlayerPath, downloadDir,
    loadSettings, updateSetting,
  } from '$lib/stores/settings';
  import { detectExternalPlayers, getDefaultDownloadDir, showInFolder, getSetting, setSetting, tmdbCheckKey } from '$lib/tauri';
  import type { ExternalPlayer } from '$lib/tauri';
  import { open } from '@tauri-apps/plugin-dialog';

  let detectedPlayers: ExternalPlayer[] = $state([]);
  let defaultDownloadDir: string = $state('');
  let tmdbKey = $state('');
  let tmdbStatus = $state<'' | 'checking' | 'saved' | 'invalid' | 'error' | 'removed'>('');
  let tmdbError = $state('');

  onMount(async () => {
    await loadSettings();
    tmdbKey = (await getSetting('tmdb_api_key').catch(() => null)) ?? '';
    try {
      detectedPlayers = await detectExternalPlayers();
    } catch (e) {
      console.error('Failed to detect players:', e);
    }
    try {
      defaultDownloadDir = await getDefaultDownloadDir();
    } catch (e) {
      console.error('Failed to get default download dir:', e);
    }
  });

  async function saveTmdb() {
    const key = tmdbKey.trim();
    if (!key) {
      await setSetting('tmdb_api_key', '').catch(() => {});
      tmdbStatus = 'removed';
      return;
    }
    tmdbStatus = 'checking';
    try {
      if (!(await tmdbCheckKey(key))) {
        tmdbStatus = 'invalid';
        return;
      }
      await setSetting('tmdb_api_key', key);
      tmdbStatus = 'saved';
    } catch (e) {
      tmdbError = String(e);
      tmdbStatus = 'error';
    }
  }

  function handleThemeChange(value: string) {
    updateSetting('theme', value);
  }

  function handlePlayerSelect(player: ExternalPlayer) {
    updateSetting('external_player', player.name);
    updateSetting('external_player_path', player.path);
  }

  async function handleBrowseDownloadDir() {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: $downloadDir || defaultDownloadDir || undefined,
      title: 'Select Download Folder',
    });
    if (selected) {
      updateSetting('download_dir', selected);
    }
  }

  function handleResetDownloadDir() {
    updateSetting('download_dir', '');
  }

  async function handleOpenDownloadDir() {
    const dir = $downloadDir || defaultDownloadDir;
    if (dir) {
      try {
        await showInFolder(dir);
      } catch {
        // folder might not exist yet
      }
    }
  }
</script>

<div class="settings-page fade-in">
  <h1 class="page-title">Settings</h1>

  <div class="settings-sections">
    <!-- General -->
    <section class="setting-section">
      <h2 class="section-title">General</h2>
      <div class="setting-row">
        <div class="setting-label">
          <span class="label-text">Theme</span>
          <span class="label-desc">Choose the app appearance</span>
        </div>
        <div class="setting-control">
          <div class="toggle-group">
            <button
              class="toggle-btn"
              class:active={$theme === 'dark'}
              onclick={() => handleThemeChange('dark')}
            >Dark</button>
            <button
              class="toggle-btn"
              class:active={$theme === 'light'}
              onclick={() => handleThemeChange('light')}
            >Light</button>
          </div>
        </div>
      </div>
    </section>

    <!-- Playback -->
    <section class="setting-section">
      <h2 class="section-title">Playback</h2>
      <div class="setting-row">
        <div class="setting-label">
          <span class="label-text">External Player</span>
          <span class="label-desc">Select a player for external playback</span>
        </div>
        <div class="setting-control">
          {#if detectedPlayers.length > 0}
            <div class="player-list">
              {#each detectedPlayers as player}
                <button
                  class="player-option"
                  class:selected={$externalPlayerPath === player.path}
                  onclick={() => handlePlayerSelect(player)}
                >
                  <span class="player-name">{player.name}</span>
                  <span class="player-path">{player.path}</span>
                </button>
              {/each}
            </div>
          {:else}
            <span class="no-players">No external players detected</span>
          {/if}
        </div>
      </div>
    </section>

    <!-- Downloads -->
    <section class="setting-section">
      <h2 class="section-title">Downloads</h2>
      <div class="setting-row">
        <div class="setting-label">
          <span class="label-text">Download Directory</span>
          <span class="label-desc">Where downloaded files are saved</span>
        </div>
        <div class="setting-control dir-control">
          <div class="dir-display" title={$downloadDir || defaultDownloadDir}>
            <svg class="dir-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
            <span class="dir-path">{$downloadDir || defaultDownloadDir || 'Not set'}</span>
            {#if !$downloadDir}
              <span class="dir-default-badge">default</span>
            {/if}
          </div>
          <div class="dir-actions">
            <button class="dir-btn" onclick={handleBrowseDownloadDir} title="Browse">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
              Browse
            </button>
            <button class="dir-btn" onclick={handleOpenDownloadDir} title="Open folder">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>
              Open
            </button>
            {#if $downloadDir}
              <button class="dir-btn reset" onclick={handleResetDownloadDir} title="Reset to default">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 105.64-12.27L1 10"/></svg>
                Reset
              </button>
            {/if}
          </div>
        </div>
      </div>
    </section>

    <!-- Artwork -->
    <section class="setting-section">
      <h2 class="section-title">Artwork</h2>
      <div class="setting-row">
        <div class="setting-label">
          <span class="label-text">TMDB API key</span>
          <span class="label-desc">
            Adds sharp backdrops, title logos, cast photos and trailers from The Movie Database.
            Get a free key at themoviedb.org → Settings → API, then paste the API key or the read access token here.
          </span>
        </div>
        <div class="setting-control tmdb-control">
          <input
            type="password"
            bind:value={tmdbKey}
            oninput={() => (tmdbStatus = '')}
            placeholder="Paste your TMDB key"
            autocomplete="off"
            spellcheck="false"
            aria-label="TMDB API key"
          />
          <button class="tmdb-save" onclick={saveTmdb} disabled={tmdbStatus === 'checking'}>
            {tmdbStatus === 'checking' ? 'Checking…' : 'Save'}
          </button>
          {#if tmdbStatus === 'saved'}<span class="tmdb-note ok">Key works. Artwork appears on title pages.</span>
          {:else if tmdbStatus === 'invalid'}<span class="tmdb-note bad">TMDB rejected this key.</span>
          {:else if tmdbStatus === 'error'}<span class="tmdb-note bad">Couldn't reach TMDB: {tmdbError}</span>
          {:else if tmdbStatus === 'removed'}<span class="tmdb-note">Key removed.</span>{/if}
        </div>
      </div>
    </section>

    <!-- About -->
    <section class="setting-section">
      <h2 class="section-title">About</h2>
      <div class="about-info">
        <div class="about-logo">J</div>
        <div>
          <p class="about-name">JoTV</p>
          <p class="about-version">Version 0.0.1</p>
          <p class="about-desc">A personal IPTV player built with Tauri + SvelteKit</p>
        </div>
      </div>
    </section>
  </div>
</div>

<style>
  .settings-page {
    display: flex;
    flex-direction: column;
    gap: 24px;
    max-width: 680px;
    padding-bottom: 40px;
  }

  .page-title {
    font-size: 24px;
    font-weight: 700;
  }

  .settings-sections {
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  .setting-section {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text);
    padding-bottom: 8px;
    border-bottom: 1px solid var(--color-border);
  }

  .setting-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
    padding: 8px 0;
  }

  .setting-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label-text {
    font-size: 14px;
    font-weight: 500;
  }

  .label-desc {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .setting-control {
    flex-shrink: 0;
  }

  .setting-control.dir-control {
    flex-shrink: 1;
  }

  .toggle-group {
    display: flex;
    gap: 4px;
    background: var(--color-surface);
    border-radius: 8px;
    padding: 3px;
  }

  .toggle-btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-muted);
    background: transparent;
  }

  .toggle-btn.active {
    color: var(--color-text);
    background: var(--color-card);
  }

  .player-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .player-option {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text);
    text-align: left;
    border: 1px solid transparent;
  }

  .player-option:hover {
    background: var(--color-hover);
  }

  .player-option.selected {
    border-color: var(--color-accent-green);
    background: rgba(78, 204, 163, 0.08);
  }

  .player-name {
    font-size: 13px;
    font-weight: 600;
  }

  .player-path {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .no-players {
    font-size: 13px;
    color: var(--color-text-muted);
  }

  .dir-control {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .dir-display {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: var(--color-surface);
    border-radius: 8px;
    border: 1px solid var(--color-border);
    min-width: 0;
  }

  .dir-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: var(--color-text-muted);
  }

  .dir-path {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
    color: var(--color-text);
  }

  .dir-default-badge {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(78, 204, 163, 0.12);
    color: var(--color-accent-green);
    flex-shrink: 0;
  }

  .dir-actions {
    display: flex;
    gap: 6px;
  }

  .dir-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 500;
    border-radius: 6px;
    background: var(--color-surface);
    color: var(--color-text);
    border: 1px solid var(--color-border);
  }

  .dir-btn:hover {
    background: var(--color-hover);
  }

  .dir-btn.reset {
    color: var(--color-text-muted);
  }

  .dir-btn :global(svg) {
    width: 14px;
    height: 14px;
  }

  .about-info {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px;
    background: var(--color-card);
    border-radius: var(--radius-card);
    border: 1px solid var(--color-border);
  }

  .about-logo {
    width: 48px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    font-weight: 800;
    color: var(--color-accent);
    background: rgba(233, 69, 96, 0.12);
    border-radius: 12px;
    flex-shrink: 0;
  }

  .about-name {
    font-size: 16px;
    font-weight: 700;
  }

  .about-version {
    font-size: 12px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }

  .about-desc {
    font-size: 12px;
    color: var(--color-text-muted);
    margin-top: 4px;
  }

  .tmdb-control { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; max-width: 520px; }
  .tmdb-control input { flex: 1; min-width: 240px; height: 38px; }
  .tmdb-save {
    height: 38px;
    padding: 0 18px;
    border-radius: var(--radius-btn);
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-weight: 700;
  }
  .tmdb-note { flex-basis: 100%; font-size: 0.8125rem; color: var(--color-text-muted); }
  .tmdb-note.ok { color: var(--color-accent-green); }
  .tmdb-note.bad { color: var(--color-accent-soft); }
</style>
