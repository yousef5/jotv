<script lang="ts">
  import { onMount } from 'svelte';
  import { downloads, loadDownloads, downloadStats, downloadSummary } from '$lib/stores/downloads';
  import {
    pauseDownload, resumeDownload, cancelDownload, clearCompletedDownloads,
    openDownloadFile, showInFolder,
  } from '$lib/tauri';
  import type { Download } from '$lib/tauri';

  onMount(() => {
    loadDownloads();
  });

  async function handlePause(d: Download) {
    await pauseDownload(d.id);
    await loadDownloads();
  }

  async function handleResume(d: Download) {
    await resumeDownload(d.id);
    await loadDownloads();
  }

  async function handleCancel(d: Download) {
    await cancelDownload(d.id);
    await loadDownloads();
  }

  async function handleClearCompleted() {
    await clearCompletedDownloads();
    await loadDownloads();
  }

  function formatBytes(bytes: number | null): string {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function formatSpeed(bps: number): string {
    if (bps <= 0) return '';
    if (bps >= 1024 * 1024) return (bps / (1024 * 1024)).toFixed(1) + ' MB/s';
    if (bps >= 1024) return (bps / 1024).toFixed(0) + ' KB/s';
    return bps + ' B/s';
  }

  function formatEta(seconds: number | null): string {
    if (seconds == null || seconds <= 0) return '';
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (h > 0) return `${h}h ${m}m`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }

  function getFilename(d: Download): string {
    if (d.file_path) {
      const parts = d.file_path.split('/');
      return parts[parts.length - 1] || d.file_path;
    }
    try {
      const urlPath = new URL(d.url).pathname;
      const segments = urlPath.split('/');
      return segments[segments.length - 1] || d.url;
    } catch {
      return d.url;
    }
  }

  function getDisplayName(d: Download): string {
    return d.channel_name || getFilename(d);
  }

  async function handleOpen(d: Download) {
    if (d.file_path) {
      try { await openDownloadFile(d.file_path); } catch (e) { console.error('Failed to open:', e); }
    }
  }

  async function handleShowInFolder(d: Download) {
    if (d.file_path) {
      try { await showInFolder(d.file_path); } catch (e) { console.error('Failed to show:', e); }
    }
  }

  let hasCompleted = $derived($downloads.some(d => d.status === 'completed'));

  // Group downloads by status for display
  let activeDownloads = $derived($downloads.filter(d => d.status === 'downloading' || d.status === 'queued'));
  let pausedDownloads = $derived($downloads.filter(d => d.status === 'paused'));
  let failedDownloads = $derived($downloads.filter(d => d.status === 'failed'));
  let completedDownloads = $derived($downloads.filter(d => d.status === 'completed'));
</script>

<div class="downloads-page fade-in">
  <!-- Stats header -->
  <div class="stats-bar">
    <div class="stat-item">
      <span class="stat-value">{$downloadSummary.activeCount}</span>
      <span class="stat-label">Active</span>
    </div>
    <div class="stat-item">
      <span class="stat-value">{$downloadSummary.completed}</span>
      <span class="stat-label">Completed</span>
    </div>
    <div class="stat-item">
      <span class="stat-value">{$downloadSummary.total}</span>
      <span class="stat-label">Total</span>
    </div>
    {#if $downloadSummary.totalSpeed > 0}
      <div class="stat-item speed">
        <span class="stat-value">{formatSpeed($downloadSummary.totalSpeed)}</span>
        <span class="stat-label">Speed</span>
      </div>
    {/if}
    <div class="stat-spacer"></div>
    {#if hasCompleted}
      <button class="btn-clear" onclick={handleClearCompleted}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
        Clear Completed
      </button>
    {/if}
  </div>

  {#if $downloads.length === 0}
    <div class="empty">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
      <p>No downloads yet</p>
      <span class="empty-hint">Download movies and series from the playlists page</span>
    </div>
  {:else}
    <div class="download-sections">
      <!-- Active downloads -->
      {#if activeDownloads.length > 0}
        <div class="section">
          <h3 class="section-title">Downloading</h3>
          <div class="download-list">
            {#each activeDownloads as d (d.id)}
              {@const stats = $downloadStats[d.id]}
              {@const pct = Math.round(d.progress * 100)}
              <div class="dl-card active">
                <div class="dl-thumb-wrap">
                  {#if d.channel_logo}
                    <img class="dl-thumb" src={d.channel_logo} alt="" />
                  {:else}
                    <div class="dl-thumb placeholder">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                    </div>
                  {/if}
                  <!-- Circular progress ring -->
                  <svg class="dl-ring" viewBox="0 0 36 36">
                    <circle class="dl-ring-bg" cx="18" cy="18" r="16" />
                    <circle class="dl-ring-fill" cx="18" cy="18" r="16"
                      stroke-dasharray="{d.progress * 100.53} 100.53"
                    />
                  </svg>
                  <span class="dl-ring-text">{pct}%</span>
                </div>
                <div class="dl-info">
                  <span class="dl-name" title={d.file_path || d.url}>{getDisplayName(d)}</span>
                  <span class="dl-filename">{getFilename(d)}</span>
                  <div class="dl-progress-bar">
                    <div class="dl-progress-fill downloading" style="width: {pct}%"></div>
                  </div>
                  <div class="dl-meta">
                    <span class="dl-size">{formatBytes(d.downloaded_bytes)} / {formatBytes(d.total_bytes)}</span>
                    {#if stats}
                      <span class="dl-speed">{formatSpeed(stats.speed_bps)}</span>
                      {#if stats.eta_seconds}
                        <span class="dl-eta">{formatEta(stats.eta_seconds)} left</span>
                      {/if}
                    {/if}
                  </div>
                </div>
                <div class="dl-actions">
                  <button class="dl-btn" onclick={() => handlePause(d)} title="Pause">
                    <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
                  </button>
                  <button class="dl-btn danger" onclick={() => handleCancel(d)} title="Cancel">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                  </button>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Paused -->
      {#if pausedDownloads.length > 0}
        <div class="section">
          <h3 class="section-title">Paused</h3>
          <div class="download-list">
            {#each pausedDownloads as d (d.id)}
              {@const pct = Math.round(d.progress * 100)}
              <div class="dl-card paused">
                <div class="dl-thumb-wrap">
                  {#if d.channel_logo}
                    <img class="dl-thumb" src={d.channel_logo} alt="" />
                  {:else}
                    <div class="dl-thumb placeholder">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
                    </div>
                  {/if}
                  <span class="dl-ring-text">{pct}%</span>
                </div>
                <div class="dl-info">
                  <span class="dl-name">{getDisplayName(d)}</span>
                  <span class="dl-filename">{getFilename(d)}</span>
                  <div class="dl-progress-bar">
                    <div class="dl-progress-fill paused" style="width: {pct}%"></div>
                  </div>
                  <div class="dl-meta">
                    <span class="dl-size">{formatBytes(d.downloaded_bytes)} / {formatBytes(d.total_bytes)}</span>
                    <span class="dl-status-tag paused">Paused</span>
                  </div>
                </div>
                <div class="dl-actions">
                  <button class="dl-btn resume" onclick={() => handleResume(d)} title="Resume">
                    <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                  </button>
                  <button class="dl-btn danger" onclick={() => handleCancel(d)} title="Cancel">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                  </button>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Failed -->
      {#if failedDownloads.length > 0}
        <div class="section">
          <h3 class="section-title failed-title">Failed</h3>
          <div class="download-list">
            {#each failedDownloads as d (d.id)}
              <div class="dl-card failed">
                <div class="dl-thumb-wrap">
                  {#if d.channel_logo}
                    <img class="dl-thumb" src={d.channel_logo} alt="" />
                  {:else}
                    <div class="dl-thumb placeholder failed">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>
                    </div>
                  {/if}
                </div>
                <div class="dl-info">
                  <span class="dl-name">{getDisplayName(d)}</span>
                  <span class="dl-filename">{getFilename(d)}</span>
                  <div class="dl-meta">
                    <span class="dl-size">{formatBytes(d.downloaded_bytes)} / {formatBytes(d.total_bytes)}</span>
                    <span class="dl-status-tag failed">Failed</span>
                  </div>
                </div>
                <div class="dl-actions">
                  <button class="dl-btn resume" onclick={() => handleResume(d)} title="Retry">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
                  </button>
                  <button class="dl-btn danger" onclick={() => handleCancel(d)} title="Remove">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                  </button>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Completed -->
      {#if completedDownloads.length > 0}
        <div class="section">
          <h3 class="section-title">Completed</h3>
          <div class="download-list">
            {#each completedDownloads as d (d.id)}
              <div class="dl-card completed">
                <div class="dl-thumb-wrap">
                  {#if d.channel_logo}
                    <img class="dl-thumb" src={d.channel_logo} alt="" />
                  {:else}
                    <div class="dl-thumb placeholder completed">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>
                    </div>
                  {/if}
                </div>
                <div class="dl-info">
                  <span class="dl-name">{getDisplayName(d)}</span>
                  <span class="dl-filename">{getFilename(d)}</span>
                  <div class="dl-meta">
                    <span class="dl-size">{formatBytes(d.total_bytes || d.downloaded_bytes)}</span>
                    <span class="dl-status-tag completed">Completed</span>
                  </div>
                </div>
                <div class="dl-actions">
                  <button class="dl-btn play" onclick={() => handleOpen(d)} title="Play">
                    <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                  </button>
                  <button class="dl-btn" onclick={() => handleShowInFolder(d)} title="Show in folder">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
                  </button>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .downloads-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding-bottom: 40px;
  }

  /* Stats bar */
  .stats-bar {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 16px 20px;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
  }

  .stat-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .stat-value {
    font-size: 20px;
    font-weight: 700;
    color: var(--color-text);
  }

  .stat-item.speed .stat-value {
    color: var(--color-accent-green);
    font-size: 16px;
  }

  .stat-label {
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .stat-spacer {
    flex: 1;
  }

  .btn-clear {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 500;
    transition: all 0.15s;
  }

  .btn-clear:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .btn-clear :global(svg) {
    width: 14px;
    height: 14px;
  }

  /* Empty state */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 80px 0;
    color: var(--color-text-muted);
  }

  .empty :global(svg) {
    width: 48px;
    height: 48px;
    opacity: 0.3;
  }

  .empty-hint {
    font-size: 12px;
    opacity: 0.5;
  }

  /* Sections */
  .download-sections {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .section-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 10px;
    padding-left: 4px;
  }

  .failed-title {
    color: var(--color-accent);
  }

  .download-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Download card */
  .dl-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    transition: border-color 0.15s;
  }

  .dl-card:hover {
    border-color: rgba(255, 255, 255, 0.1);
  }

  .dl-card.active {
    border-color: rgba(166, 227, 161, 0.2);
  }

  /* Thumbnail */
  .dl-thumb-wrap {
    position: relative;
    width: 52px;
    height: 52px;
    flex-shrink: 0;
  }

  .dl-thumb {
    width: 52px;
    height: 52px;
    border-radius: 10px;
    object-fit: cover;
    background: var(--color-surface);
  }

  .dl-thumb.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-surface);
    border-radius: 10px;
    color: var(--color-text-muted);
  }

  .dl-thumb.placeholder :global(svg) {
    width: 20px;
    height: 20px;
  }

  .dl-thumb.placeholder.completed {
    color: var(--color-accent-green);
    background: rgba(166, 227, 161, 0.08);
  }

  .dl-thumb.placeholder.failed {
    color: var(--color-accent);
    background: rgba(233, 69, 96, 0.08);
  }

  /* Progress ring */
  .dl-ring {
    position: absolute;
    inset: -2px;
    width: 56px;
    height: 56px;
    transform: rotate(-90deg);
  }

  .dl-ring-bg {
    fill: none;
    stroke: rgba(255, 255, 255, 0.06);
    stroke-width: 2.5;
  }

  .dl-ring-fill {
    fill: none;
    stroke: var(--color-accent-green);
    stroke-width: 2.5;
    stroke-linecap: round;
    transition: stroke-dasharray 0.3s ease;
  }

  .dl-ring-text {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    color: var(--color-text);
  }

  /* Info */
  .dl-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .dl-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dl-filename {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0.6;
  }

  /* Progress bar */
  .dl-progress-bar {
    width: 100%;
    height: 4px;
    background: var(--color-surface);
    border-radius: 2px;
    overflow: hidden;
    margin: 2px 0;
  }

  .dl-progress-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.25s ease;
  }

  .dl-progress-fill.downloading {
    background: linear-gradient(90deg, var(--color-accent-green), #5df0a0);
    animation: progress-shimmer 1.5s ease infinite;
    background-size: 200% 100%;
  }

  .dl-progress-fill.paused {
    background: var(--color-accent-yellow, #f9e2af);
  }

  @keyframes progress-shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }

  /* Meta row */
  .dl-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 11px;
  }

  .dl-size {
    color: var(--color-text-muted);
  }

  .dl-speed {
    color: var(--color-accent-green);
    font-weight: 600;
  }

  .dl-eta {
    color: var(--color-accent-yellow, #f9e2af);
  }

  .dl-status-tag {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .dl-status-tag.completed {
    background: rgba(166, 227, 161, 0.1);
    color: var(--color-accent-green);
  }

  .dl-status-tag.paused {
    background: rgba(249, 226, 175, 0.1);
    color: var(--color-accent-yellow, #f9e2af);
  }

  .dl-status-tag.failed {
    background: rgba(233, 69, 96, 0.1);
    color: var(--color-accent);
  }

  /* Action buttons */
  .dl-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .dl-btn {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text-muted);
    transition: all 0.15s;
  }

  .dl-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .dl-btn.danger {
    color: var(--color-accent);
  }

  .dl-btn.danger:hover {
    background: rgba(233, 69, 96, 0.12);
  }

  .dl-btn.resume {
    color: var(--color-accent-green);
  }

  .dl-btn.resume:hover {
    background: rgba(166, 227, 161, 0.12);
  }

  .dl-btn.play {
    color: var(--color-accent-green);
  }

  .dl-btn.play:hover {
    background: rgba(166, 227, 161, 0.12);
  }

  .dl-btn :global(svg) {
    width: 16px;
    height: 16px;
  }
</style>
