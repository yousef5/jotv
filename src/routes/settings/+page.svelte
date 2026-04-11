<script lang="ts">
  import { onMount } from 'svelte';
  import {
    theme, externalPlayer, externalPlayerPath, downloadDir,
    loadSettings, updateSetting,
  } from '$lib/stores/settings';
  import { detectExternalPlayers } from '$lib/tauri';
  import type { ExternalPlayer } from '$lib/tauri';

  let detectedPlayers: ExternalPlayer[] = $state([]);

  onMount(async () => {
    await loadSettings();
    try {
      detectedPlayers = await detectExternalPlayers();
    } catch (e) {
      console.error('Failed to detect players:', e);
    }
  });

  function handleThemeChange(value: string) {
    updateSetting('theme', value);
  }

  function handlePlayerSelect(player: ExternalPlayer) {
    updateSetting('external_player', player.name);
    updateSetting('external_player_path', player.path);
  }

  function handleDownloadDir(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    updateSetting('download_dir', value);
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
        <div class="setting-control">
          <input
            type="text"
            value={$downloadDir}
            placeholder="/path/to/downloads"
            onchange={handleDownloadDir}
            class="dir-input"
          />
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

  .dir-input {
    width: 280px;
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
</style>
