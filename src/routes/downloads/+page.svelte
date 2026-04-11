<script lang="ts">
  import { onMount } from 'svelte';
  import { downloads, loadDownloads } from '$lib/stores/downloads';
  import {
    pauseDownload, resumeDownload, cancelDownload, clearCompletedDownloads,
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

  function statusColor(status: string): string {
    switch (status) {
      case 'completed': return 'badge-green';
      case 'downloading': return 'badge-purple';
      case 'queued': return 'badge-yellow';
      case 'paused': return 'badge-yellow';
      case 'failed': return 'badge-red';
      default: return 'badge-purple';
    }
  }

  function formatBytes(bytes: number | null): string {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  let hasCompleted = $derived($downloads.some(d => d.status === 'completed'));
</script>

<div class="downloads-page fade-in">
  <div class="page-header">
    <h1 class="page-title">Downloads</h1>
    {#if hasCompleted}
      <button class="btn-ghost" onclick={handleClearCompleted}>Clear Completed</button>
    {/if}
  </div>

  {#if $downloads.length === 0}
    <div class="empty">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
      <p>No downloads yet.</p>
    </div>
  {:else}
    <div class="download-list">
      {#each $downloads as d (d.id)}
        <div class="download-item card">
          <div class="download-info">
            <div class="download-top">
              <span class="download-url" title={d.url}>{d.url}</span>
              <span class="badge {statusColor(d.status)}">{d.status}</span>
            </div>
            <div class="progress-bar">
              <div class="progress-fill" style="width: {d.progress * 100}%"></div>
            </div>
            <div class="download-bottom">
              <span class="download-size">
                {formatBytes(d.downloaded_bytes)} / {formatBytes(d.total_bytes)}
              </span>
              <span class="download-percent">{Math.round(d.progress * 100)}%</span>
            </div>
          </div>
          <div class="download-actions">
            {#if d.status === 'downloading' || d.status === 'queued'}
              <button class="ctrl-btn" onclick={() => handlePause(d)} title="Pause">
                <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
              </button>
            {/if}
            {#if d.status === 'paused'}
              <button class="ctrl-btn" onclick={() => handleResume(d)} title="Resume">
                <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
              </button>
            {/if}
            {#if d.status !== 'completed'}
              <button class="ctrl-btn danger" onclick={() => handleCancel(d)} title="Cancel">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
              </button>
            {/if}
          </div>
        </div>
      {/each}
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

  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
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

  .download-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .download-item {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px;
  }

  .download-item:hover {
    transform: none;
  }

  .download-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .download-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .download-url {
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  .progress-bar {
    width: 100%;
    height: 4px;
    background: var(--color-surface);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--color-accent-green);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .download-bottom {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .download-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .ctrl-btn {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text);
  }

  .ctrl-btn:hover {
    background: var(--color-hover);
  }

  .ctrl-btn.danger {
    color: var(--color-accent);
  }

  .ctrl-btn :global(svg) {
    width: 16px;
    height: 16px;
  }
</style>
