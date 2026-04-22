<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    checkYtdlp, getSocialVideoInfo, startSocialDownload, cancelSocialDownload,
    getDefaultDownloadDir, openDownloadFile, showInFolder,
    getSocialDownloads, clearSocialDownloads, getSetting, setSetting,
  } from '$lib/tauri';
  import type { SocialVideoInfo, SocialDownloadProgress, SocialFormat } from '$lib/tauri';

  // yt-dlp status
  let ytdlpVersion = $state<string | null>(null);
  let ytdlpMissing = $state(false);

  // URL input
  let urlInput = $state('');
  let fetchingInfo = $state(false);
  let fetchError = $state('');

  // Video info
  let videoInfo = $state<SocialVideoInfo | null>(null);
  let selectedFormat = $state<string>('bestvideo+bestaudio/best');
  let qualityOpen = $state(false);

  // Output directory
  let outputDir = $state('');

  // Downloads list
  interface SocialDl {
    id: string;
    title: string;
    thumbnail: string | null;
    platform: string;
    progress: number;
    speed: string | null;
    eta: string | null;
    status: string; // downloading, completed, failed, cancelled
    filename: string | null;
    downloaded_bytes: number;
    total_bytes: number | null;
  }

  let socialDownloads = $state<SocialDl[]>([]);
  let unlistenProgress: (() => void) | null = null;

  onMount(async () => {
    // Check yt-dlp
    try {
      ytdlpVersion = await checkYtdlp();
    } catch {
      ytdlpMissing = true;
    }

    // Restore saved output dir, fallback to default
    try {
      const saved = await getSetting('social_download_dir');
      outputDir = saved || await getDefaultDownloadDir();
    } catch {}

    // Load history from DB
    try {
      const history = await getSocialDownloads();
      socialDownloads = history.map(h => ({
        id: h.id,
        title: h.title,
        thumbnail: h.thumbnail,
        platform: h.platform,
        progress: h.progress,
        speed: null,
        eta: null,
        status: h.status === 'downloading' ? 'failed' : h.status, // stale 'downloading' = crashed
        filename: h.file_path,
        downloaded_bytes: h.downloaded_bytes,
        total_bytes: h.total_bytes,
      }));
    } catch {}

    // Listen for progress
    unlistenProgress = await listen<SocialDownloadProgress>('social-download-progress', (event) => {
      const p = event.payload;
      socialDownloads = socialDownloads.map(d =>
        d.id === p.download_id
          ? {
              ...d,
              progress: p.progress,
              speed: p.speed,
              eta: p.eta,
              status: p.status,
              filename: p.filename || d.filename,
              downloaded_bytes: p.downloaded_bytes,
              total_bytes: p.total_bytes ?? d.total_bytes,
            }
          : d
      );
    });
  });

  onDestroy(() => {
    unlistenProgress?.();
  });

  async function handleFetchInfo() {
    if (!urlInput.trim()) return;
    fetchingInfo = true;
    fetchError = '';
    videoInfo = null;
    try {
      videoInfo = await getSocialVideoInfo(urlInput.trim());
      if (videoInfo.formats.length > 0) {
        selectedFormat = videoInfo.formats[0].format_id;
      }
    } catch (e) {
      fetchError = String(e);
    } finally {
      fetchingInfo = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleFetchInfo();
  }

  async function handlePickFolder() {
    const selected = await open({ directory: true, title: 'Select download folder' });
    if (selected) {
      outputDir = selected as string;
      setSetting('social_download_dir', outputDir).catch(() => {});
    }
  }

  async function handleStartDownload() {
    if (!videoInfo) return;
    const dlId = `social-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    socialDownloads = [{
      id: dlId,
      title: videoInfo.title,
      thumbnail: videoInfo.thumbnail,
      platform: videoInfo.platform,
      progress: 0,
      speed: null,
      eta: null,
      status: 'downloading',
      filename: null,
      downloaded_bytes: 0,
      total_bytes: null,
    }, ...socialDownloads];

    // Clear the form for next download
    const url = videoInfo.url;
    const fmt = selectedFormat;
    const title = videoInfo.title;
    const thumbnail = videoInfo.thumbnail;
    const platform = videoInfo.platform;
    const formatLabel = videoInfo.formats.find(f => f.format_id === fmt)?.label || null;
    videoInfo = null;
    urlInput = '';

    try {
      await startSocialDownload(url, fmt, outputDir, dlId, title, thumbnail, platform, formatLabel);
    } catch (e) {
      socialDownloads = socialDownloads.map(d =>
        d.id === dlId ? { ...d, status: 'failed' } : d
      );
    }
  }

  async function handleCancel(dlId: string) {
    try {
      await cancelSocialDownload(dlId);
    } catch {}
    socialDownloads = socialDownloads.map(d =>
      d.id === dlId ? { ...d, status: 'cancelled' } : d
    );
  }

  function handleRemove(dlId: string) {
    socialDownloads = socialDownloads.filter(d => d.id !== dlId);
    // Also remove from DB (fire and forget)
    clearSocialDownloads().catch(() => {});
  }

  async function handleClearHistory() {
    await clearSocialDownloads();
    socialDownloads = socialDownloads.filter(d => d.status === 'downloading');
  }

  async function handleOpenFile(dl: SocialDl) {
    if (dl.filename) {
      try { await openDownloadFile(dl.filename); } catch {}
    }
  }

  async function handleShowFolder(dl: SocialDl) {
    if (dl.filename) {
      try { await showInFolder(dl.filename); } catch {}
    }
  }

  function formatDuration(secs: number | null): string {
    if (!secs) return '';
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
    return `${m}:${String(s).padStart(2, '0')}`;
  }

  function formatViews(count: number | null): string {
    if (!count) return '';
    if (count >= 1_000_000) return (count / 1_000_000).toFixed(1) + 'M views';
    if (count >= 1_000) return (count / 1_000).toFixed(1) + 'K views';
    return count + ' views';
  }

  function formatBytes(bytes: number | null): string {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function platformColor(platform: string): string {
    switch (platform) {
      case 'YouTube': return '#ff0000';
      case 'Instagram': return '#e1306c';
      case 'Facebook': return '#1877f2';
      case 'TikTok': return '#00f2ea';
      case 'Twitter/X': return '#1da1f2';
      case 'Reddit': return '#ff4500';
      case 'Twitch': return '#9146ff';
      default: return 'var(--color-accent)';
    }
  }

  let activeCount = $derived(socialDownloads.filter(d => d.status === 'downloading').length);
  let completedCount = $derived(socialDownloads.filter(d => d.status === 'completed').length);
  let hasClearable = $derived(socialDownloads.some(d => d.status === 'completed' || d.status === 'failed' || d.status === 'cancelled'));
</script>

<div class="social-page fade-in">
  <!-- Header -->
  <div class="page-header">
    <div class="header-left">
      <h1 class="page-title">Social Downloader</h1>
      <span class="page-subtitle">YouTube, Instagram, Facebook, TikTok, Twitter & more</span>
    </div>
    {#if ytdlpVersion}
      <span class="ytdlp-badge">yt-dlp {ytdlpVersion}</span>
    {/if}
  </div>

  {#if ytdlpMissing}
    <div class="missing-card">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      <div class="missing-text">
        <h3>yt-dlp not found</h3>
        <p>Install yt-dlp to enable social media downloads:</p>
        <code>pip install yt-dlp</code>
        <span class="missing-hint">or: sudo pacman -S yt-dlp / brew install yt-dlp</span>
      </div>
    </div>
  {:else}
    <!-- URL Input -->
    <div class="url-section">
      <div class="url-input-wrap">
        <svg class="url-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
        <input
          type="text"
          class="url-input"
          placeholder="Paste URL from YouTube, Instagram, Facebook, TikTok, Twitter..."
          bind:value={urlInput}
          onkeydown={handleKeydown}
          disabled={fetchingInfo}
        />
        <button class="url-btn" onclick={handleFetchInfo} disabled={fetchingInfo || !urlInput.trim()}>
          {#if fetchingInfo}
            <svg class="spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 11-6.219-8.56"/></svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="21 21 15 15"/><circle cx="10" cy="10" r="7"/></svg>
          {/if}
          {fetchingInfo ? 'Fetching...' : 'Fetch'}
        </button>
      </div>
      {#if fetchError}
        <div class="fetch-error">{fetchError}</div>
      {/if}
    </div>

    <!-- Video Preview -->
    {#if videoInfo}
      <div class="preview-card">
        <div class="preview-main">
          {#if videoInfo.thumbnail}
            <div class="preview-thumb-wrap">
              <img class="preview-thumb" src={videoInfo.thumbnail} alt="" />
              {#if videoInfo.duration}
                <span class="preview-duration">{formatDuration(videoInfo.duration)}</span>
              {/if}
            </div>
          {/if}
          <div class="preview-info">
            <span class="preview-platform" style="color: {platformColor(videoInfo.platform)}">{videoInfo.platform}</span>
            <h3 class="preview-title">{videoInfo.title}</h3>
            <div class="preview-meta">
              {#if videoInfo.uploader}
                <span class="preview-uploader">{videoInfo.uploader}</span>
              {/if}
              {#if videoInfo.view_count}
                <span class="preview-views">{formatViews(videoInfo.view_count)}</span>
              {/if}
            </div>
          </div>
        </div>

        <div class="preview-options">
          <!-- Quality dropdown -->
          <div class="option-group">
            <span class="option-label">Quality</span>
            <div class="dropdown-wrap">
              <button class="dropdown-trigger" onclick={() => qualityOpen = !qualityOpen}>
                <span class="dropdown-value">{videoInfo.formats.find(f => f.format_id === selectedFormat)?.label || 'Select quality'}</span>
                <svg class="dropdown-arrow" class:open={qualityOpen} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"/></svg>
              </button>
              {#if qualityOpen}
                <div class="dropdown-menu">
                  {#each videoInfo.formats as fmt (fmt.format_id)}
                    <button
                      class="dropdown-item"
                      class:active={selectedFormat === fmt.format_id}
                      onclick={() => { selectedFormat = fmt.format_id; qualityOpen = false; }}
                    >
                      <span class="dropdown-item-label">{fmt.label}</span>
                      {#if fmt.filesize}
                        <span class="dropdown-item-size">{formatBytes(fmt.filesize)}</span>
                      {/if}
                      {#if selectedFormat === fmt.format_id}
                        <svg class="dropdown-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>

          <!-- Output folder -->
          <div class="option-group">
            <span class="option-label">Save to</span>
            <div class="folder-pick">
              <span class="folder-path" title={outputDir}>{outputDir}</span>
              <button class="folder-btn" onclick={handlePickFolder}>Browse</button>
            </div>
          </div>

          <!-- Download button -->
          <button class="download-btn" onclick={handleStartDownload}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
            Download
          </button>
        </div>
      </div>
    {/if}

    <!-- Downloads list -->
    {#if socialDownloads.length > 0}
      <div class="dl-section">
        <div class="dl-section-header">
          <h2 class="dl-section-title">Downloads</h2>
          <div class="dl-section-stats">
            {#if activeCount > 0}
              <span class="dl-stat active">{activeCount} active</span>
            {/if}
            {#if completedCount > 0}
              <span class="dl-stat completed">{completedCount} completed</span>
            {/if}
            {#if hasClearable}
              <button class="clear-history-btn" onclick={handleClearHistory}>Clear History</button>
            {/if}
          </div>
        </div>

        <div class="dl-list">
          {#each socialDownloads as dl (dl.id)}
            <div class="sdl-card" class:completed={dl.status === 'completed'} class:failed={dl.status === 'failed'}>
              <div class="sdl-thumb-wrap">
                {#if dl.thumbnail}
                  <img class="sdl-thumb" src={dl.thumbnail} alt="" />
                {:else}
                  <div class="sdl-thumb placeholder">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                  </div>
                {/if}
                {#if dl.status === 'downloading'}
                  <div class="sdl-progress-ring">
                    <svg viewBox="0 0 36 36">
                      <circle class="ring-bg" cx="18" cy="18" r="16" />
                      <circle class="ring-fill" cx="18" cy="18" r="16"
                        stroke-dasharray="{dl.progress * 100.53} 100.53" />
                    </svg>
                    <span class="ring-text">{Math.round(dl.progress * 100)}%</span>
                  </div>
                {:else if dl.status === 'completed'}
                  <div class="sdl-status-icon completed">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
                  </div>
                {:else if dl.status === 'failed' || dl.status === 'cancelled'}
                  <div class="sdl-status-icon failed">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                  </div>
                {/if}
              </div>

              <div class="sdl-info">
                <div class="sdl-top">
                  <span class="sdl-platform" style="color: {platformColor(dl.platform)}">{dl.platform}</span>
                  <span class="sdl-title">{dl.title}</span>
                </div>

                {#if dl.status === 'downloading'}
                  <div class="sdl-bar">
                    <div class="sdl-bar-fill" style="width: {dl.progress * 100}%"></div>
                  </div>
                  <div class="sdl-meta">
                    <span class="sdl-size">{formatBytes(dl.downloaded_bytes)}{dl.total_bytes ? ` / ${formatBytes(dl.total_bytes)}` : ''}</span>
                    {#if dl.speed}
                      <span class="sdl-speed">{dl.speed}</span>
                    {/if}
                    {#if dl.eta}
                      <span class="sdl-eta">ETA {dl.eta}</span>
                    {/if}
                  </div>
                {:else if dl.status === 'completed'}
                  <div class="sdl-meta">
                    <span class="sdl-done">Download complete</span>
                    {#if dl.total_bytes}
                      <span class="sdl-size">{formatBytes(dl.total_bytes)}</span>
                    {/if}
                  </div>
                {:else}
                  <div class="sdl-meta">
                    <span class="sdl-fail">{dl.status === 'cancelled' ? 'Cancelled' : 'Failed'}</span>
                  </div>
                {/if}
              </div>

              <div class="sdl-actions">
                {#if dl.status === 'completed' && dl.filename}
                  <button class="sdl-btn play" onclick={() => handleOpenFile(dl)} title="Play">
                    <svg viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
                  </button>
                  <button class="sdl-btn" onclick={() => handleShowFolder(dl)} title="Show in folder">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
                  </button>
                {/if}
                {#if dl.status === 'downloading'}
                  <button class="sdl-btn danger" onclick={() => handleCancel(dl.id)} title="Cancel">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                  </button>
                {/if}
                {#if dl.status === 'completed' || dl.status === 'failed' || dl.status === 'cancelled'}
                  <button class="sdl-btn dim" onclick={() => handleRemove(dl.id)} title="Remove">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .social-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding-bottom: 40px;
    max-width: 900px;
  }

  /* Header */
  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .header-left {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .page-title {
    font-size: 24px;
    font-weight: 700;
  }

  .page-subtitle {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .ytdlp-badge {
    font-size: 11px;
    padding: 4px 10px;
    border-radius: 6px;
    background: rgba(166, 227, 161, 0.1);
    color: var(--color-accent-green);
    font-weight: 600;
  }

  /* Missing yt-dlp */
  .missing-card {
    display: flex;
    gap: 20px;
    padding: 24px;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    align-items: flex-start;
  }

  .missing-card > :global(svg) {
    width: 40px;
    height: 40px;
    color: var(--color-accent-yellow, #f9e2af);
    flex-shrink: 0;
  }

  .missing-text {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .missing-text h3 {
    font-size: 16px;
    font-weight: 600;
  }

  .missing-text p {
    font-size: 13px;
    color: var(--color-text-muted);
  }

  .missing-text code {
    font-size: 13px;
    padding: 8px 14px;
    background: var(--color-surface);
    border-radius: 8px;
    color: var(--color-accent-green);
    font-family: monospace;
    display: inline-block;
  }

  .missing-hint {
    font-size: 11px;
    color: var(--color-text-muted);
    opacity: 0.6;
  }

  /* URL input */
  .url-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .url-input-wrap {
    display: flex;
    align-items: center;
    gap: 0;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 4px;
    transition: border-color 0.15s;
  }

  .url-input-wrap:focus-within {
    border-color: var(--color-accent);
  }

  .url-icon {
    width: 18px;
    height: 18px;
    margin: 0 10px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .url-input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: 14px;
    padding: 10px 0;
    outline: none;
  }

  .url-input::placeholder {
    color: var(--color-text-muted);
    opacity: 0.5;
  }

  .url-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 18px;
    border-radius: 8px;
    background: var(--color-accent);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    transition: background 0.15s;
    flex-shrink: 0;
  }

  .url-btn:hover:not(:disabled) {
    background: #c73850;
  }

  .url-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .url-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  .spin {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .fetch-error {
    font-size: 12px;
    color: var(--color-accent);
    padding: 8px 14px;
    background: rgba(233, 69, 96, 0.08);
    border-radius: 8px;
  }

  /* Preview card */
  .preview-card {
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
  }

  .preview-main {
    display: flex;
    gap: 16px;
    padding: 16px;
  }

  .preview-thumb-wrap {
    position: relative;
    width: 200px;
    flex-shrink: 0;
    border-radius: 8px;
    overflow: hidden;
  }

  .preview-thumb {
    width: 100%;
    aspect-ratio: 16/9;
    object-fit: cover;
    display: block;
  }

  .preview-duration {
    position: absolute;
    bottom: 6px;
    right: 6px;
    font-size: 11px;
    font-weight: 600;
    color: #fff;
    background: rgba(0, 0, 0, 0.8);
    padding: 2px 6px;
    border-radius: 4px;
  }

  .preview-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .preview-platform {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .preview-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text);
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .preview-meta {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .preview-uploader {
    font-weight: 500;
  }

  /* Options */
  .preview-options {
    display: flex;
    gap: 12px;
    align-items: flex-end;
    padding: 12px 16px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
  }

  .option-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }

  .option-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .dropdown-wrap {
    position: relative;
  }

  .dropdown-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s;
  }

  .dropdown-trigger:hover {
    border-color: rgba(255, 255, 255, 0.15);
  }

  .dropdown-value {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dropdown-arrow {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: transform 0.2s;
  }

  .dropdown-arrow.open {
    transform: rotate(180deg);
  }

  .dropdown-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 50;
    background: var(--color-sidebar);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 4px;
    max-height: 260px;
    overflow-y: auto;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--color-text);
    background: transparent;
    text-align: left;
    transition: background 0.1s;
  }

  .dropdown-item:hover {
    background: var(--color-hover);
  }

  .dropdown-item.active {
    background: rgba(233, 69, 96, 0.1);
    color: var(--color-accent);
  }

  .dropdown-item-label {
    flex: 1;
    font-weight: 500;
  }

  .dropdown-item-size {
    font-size: 11px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .dropdown-check {
    width: 14px;
    height: 14px;
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .folder-pick {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .folder-path {
    flex: 1;
    font-size: 12px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 8px 10px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
  }

  .folder-btn {
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text);
    font-size: 12px;
    font-weight: 500;
    border: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .folder-btn:hover {
    background: var(--color-hover);
  }

  .download-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    border-radius: 8px;
    background: var(--color-accent-green);
    color: #1a1a2e;
    font-size: 14px;
    font-weight: 700;
    flex-shrink: 0;
    transition: all 0.15s;
  }

  .download-btn:hover {
    transform: scale(1.03);
    filter: brightness(1.1);
  }

  .download-btn :global(svg) {
    width: 18px;
    height: 18px;
  }

  /* Downloads section */
  .dl-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .dl-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .dl-section-title {
    font-size: 16px;
    font-weight: 600;
  }

  .dl-section-stats {
    display: flex;
    gap: 8px;
  }

  .dl-stat {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 4px;
  }

  .dl-stat.active {
    background: rgba(166, 227, 161, 0.1);
    color: var(--color-accent-green);
  }

  .dl-stat.completed {
    background: rgba(166, 227, 161, 0.06);
    color: var(--color-text-muted);
  }

  .clear-history-btn {
    font-size: 11px;
    font-weight: 500;
    padding: 3px 10px;
    border-radius: 4px;
    background: var(--color-surface);
    color: var(--color-text-muted);
    transition: all 0.15s;
  }

  .clear-history-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .dl-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Social download card */
  .sdl-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
  }

  .sdl-card.completed {
    border-color: rgba(166, 227, 161, 0.15);
  }

  .sdl-card.failed {
    border-color: rgba(233, 69, 96, 0.15);
  }

  .sdl-thumb-wrap {
    position: relative;
    width: 64px;
    height: 48px;
    border-radius: 8px;
    overflow: hidden;
    flex-shrink: 0;
  }

  .sdl-thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .sdl-thumb.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-surface);
    color: var(--color-text-muted);
  }

  .sdl-thumb.placeholder :global(svg) {
    width: 20px;
    height: 20px;
  }

  .sdl-progress-ring {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
  }

  .sdl-progress-ring svg {
    position: absolute;
    width: 36px;
    height: 36px;
    transform: rotate(-90deg);
  }

  .ring-bg {
    fill: none;
    stroke: rgba(255, 255, 255, 0.1);
    stroke-width: 2.5;
  }

  .ring-fill {
    fill: none;
    stroke: var(--color-accent-green);
    stroke-width: 2.5;
    stroke-linecap: round;
    transition: stroke-dasharray 0.3s ease;
  }

  .ring-text {
    font-size: 9px;
    font-weight: 700;
    color: #fff;
    z-index: 1;
  }

  .sdl-status-icon {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
  }

  .sdl-status-icon :global(svg) {
    width: 20px;
    height: 20px;
  }

  .sdl-status-icon.completed {
    color: var(--color-accent-green);
  }

  .sdl-status-icon.failed {
    color: var(--color-accent);
  }

  .sdl-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .sdl-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sdl-platform {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    flex-shrink: 0;
  }

  .sdl-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sdl-bar {
    width: 100%;
    height: 3px;
    background: var(--color-surface);
    border-radius: 2px;
    overflow: hidden;
  }

  .sdl-bar-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--color-accent-green), #5df0a0);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .sdl-meta {
    display: flex;
    gap: 12px;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .sdl-speed {
    color: var(--color-accent-green);
    font-weight: 600;
  }

  .sdl-eta {
    color: var(--color-accent-yellow, #f9e2af);
  }

  .sdl-done {
    color: var(--color-accent-green);
    font-weight: 500;
  }

  .sdl-fail {
    color: var(--color-accent);
    font-weight: 500;
  }

  .sdl-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .sdl-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text-muted);
    transition: all 0.15s;
  }

  .sdl-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .sdl-btn.play {
    color: var(--color-accent-green);
  }

  .sdl-btn.danger {
    color: var(--color-accent);
  }

  .sdl-btn.dim {
    opacity: 0.5;
  }

  .sdl-btn.dim:hover {
    opacity: 1;
  }

  .sdl-btn :global(svg) {
    width: 15px;
    height: 15px;
  }
</style>
