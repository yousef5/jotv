<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import AddPlaylistModal from '$lib/components/AddPlaylistModal.svelte';
  import {
    deletePlaylist, exportPlaylist, getContentTypeCounts,
    getDefaultDownloadDir, showInFolder,
  } from '$lib/tauri';
  import type { Playlist, ContentTypeCount } from '$lib/tauri';
  import { playlists, loadPlaylists } from '$lib/stores/playlists';
  import { syncState, syncPlaylists } from '$lib/stores/sync';
  import { nowPlaying, stopLive } from '$lib/stores/live';
  import { accounts, loadAccount, forgetAccount } from '$lib/stores/account';
  import type { XtreamAccountInfo } from '$lib/tauri';

  let showAdd = $state(false);
  let loaded = $state(false);
  let counts = $state<Record<number, ContentTypeCount[]>>({});
  let refreshingId = $state<number | null>(null);
  let confirmDelete = $state<number | null>(null);
  let busy = $state<number | null>(null);
  let notice = $state<{ text: string; path?: string } | null>(null);
  let now = $state(Date.now());

  let confirmTimer: ReturnType<typeof setTimeout> | undefined;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  const nf = new Intl.NumberFormat('en-US');

  onMount(() => {
    load();
    const tick = setInterval(() => (now = Date.now()), 30_000);
    return () => {
      clearInterval(tick);
      clearTimeout(confirmTimer);
      clearTimeout(noticeTimer);
    };
  });

  // Refresh counts after any sync (launch sync, dashboard button, or here)
  let seenVersion = $syncState.version;
  $effect(() => {
    if ($syncState.version !== seenVersion) {
      seenVersion = $syncState.version;
      for (const p of $playlists) loadCounts(p.id);
    }
  });

  async function load() {
    await loadPlaylists();
    loaded = true;
    for (const p of $playlists) {
      loadCounts(p.id);
      loadAccount(p);
    }
  }

  function loadCounts(id: number) {
    getContentTypeCounts(id).then((c) => (counts = { ...counts, [id]: c })).catch(() => {});
  }


  function count(id: number, type: string): number {
    return counts[id]?.find((c) => c.content_type === type)?.count ?? 0;
  }

  function host(url: string): string {
    try { return new URL(url).host; } catch { return url.split('/').pop() ?? url; }
  }

  function sourceLabel(type: string): string {
    return type === 'xtream' ? 'Xtream Codes' : type === 'm3u_url' ? 'M3U link' : 'M3U file';
  }

  function updatedAgo(ts: string | null): string {
    if (!ts) return 'Never updated';
    const t = Date.parse(ts.replace(' ', 'T') + 'Z');
    if (isNaN(t)) return 'Never updated';
    const s = Math.max(0, (now - t) / 1000);
    if (s < 60) return 'Updated just now';
    if (s < 3600) return `Updated ${Math.floor(s / 60)}m ago`;
    if (s < 86400) return `Updated ${Math.floor(s / 3600)}h ago`;
    return `Updated ${Math.floor(s / 86400)}d ago`;
  }

  function expiry(info: XtreamAccountInfo): { days: number | null; date: string } {
    const n = parseInt(info.exp_date ?? '');
    if (isNaN(n)) return { days: null, date: 'No expiry' };
    return {
      days: Math.ceil((n - Date.now() / 1000) / 86400),
      date: new Date(n * 1000).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' }),
    };
  }

  function flash(text: string, path?: string) {
    notice = { text, path };
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), 6000);
  }

  async function refresh(p: Playlist) {
    if ($syncState.running) return;
    refreshingId = p.id;
    await syncPlaylists([p]);
    refreshingId = null;
    flash($syncState.error ? `Refresh failed: ${$syncState.error}` : `${p.name} is up to date`);
  }

  async function exportM3u(p: Playlist) {
    busy = p.id;
    try {
      const dir = await getDefaultDownloadDir();
      const path = `${dir.replace(/[/\\]$/, '')}/${p.name.replace(/[^\p{L}\p{N}_-]+/gu, '_')}.m3u`;
      await exportPlaylist(p.id, path);
      flash(`Exported to ${path}`, path);
    } catch (e) {
      flash(`Export failed: ${e}`);
    } finally {
      busy = null;
    }
  }

  function askDelete(id: number) {
    confirmDelete = id;
    clearTimeout(confirmTimer);
    confirmTimer = setTimeout(() => (confirmDelete = null), 4000);
  }

  async function remove(p: Playlist) {
    confirmDelete = null;
    busy = p.id;
    try {
      if ($nowPlaying?.channel.playlist_id === p.id) await stopLive();
      await deletePlaylist(p.id);
      forgetAccount(p.id);
      await loadPlaylists();
      flash(`Deleted ${p.name}`);
    } catch (e) {
      flash(`Delete failed: ${e}`);
    } finally {
      busy = null;
    }
  }

  async function onAdded() {
    showAdd = false;
    await load();
  }
</script>

<div class="manager">
  <header class="head">
    <div>
      <h1>Playlists</h1>
      <p>Your IPTV accounts and M3U sources. Everything in Live TV, Movies and Series comes from here.</p>
    </div>
    <button class="btn-red" onclick={() => (showAdd = true)}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
      Add playlist
    </button>
  </header>

  {#if loaded && !$playlists.length}
    <section class="empty">
      <span class="kicker"><b>J</b>TV</span>
      <h2>Add your first playlist</h2>
      <p>Sign in with Xtream Codes, paste an M3U link, or pick an M3U file. JoTV imports the channels, films and series and keeps them updated.</p>
      <button class="btn-red big" onclick={() => (showAdd = true)}>Add playlist</button>
    </section>
  {:else}
    <ul class="list">
      {#each $playlists as p (p.id)}
        {@const info = $accounts[p.id]}
        {@const exp = info ? expiry(info) : null}
        {@const isRefreshing = refreshingId === p.id && $syncState.running}
        <li class="item" class:dim={busy === p.id}>
          <div class="identity">
            <div class="title-row">
              <h2 dir="auto">{p.name}</h2>
              <span class="type">{sourceLabel(p.source_type)}</span>
            </div>
            <p class="source">
              <span>{p.source_type === 'm3u_file' ? 'Local file' : host(p.source_url)}</span>
              {#if p.xtream_username}<span class="sep">·</span><span>{p.xtream_username}</span>{/if}
              <span class="sep">·</span>
              <span>{updatedAgo(p.last_updated_at)}</span>
            </p>

            {#if info}
              <div class="account">
                <span class="state" class:ok={info.status === 'Active'}><i></i>{info.status ?? 'Unknown'}</span>
                <span>{info.active_cons ?? '0'} of {info.max_connections ?? '?'} streams in use</span>
                {#if exp}
                  <span class:warn={exp.days !== null && exp.days <= 7 && exp.days > 0} class:bad={exp.days !== null && exp.days <= 0}>
                    {#if exp.days === null}No expiry{:else if exp.days <= 0}Expired {exp.date}{:else}{exp.days} days left · {exp.date}{/if}
                  </span>
                {/if}
              </div>
            {/if}
          </div>

          <nav class="sections" aria-label={`Open ${p.name}`}>
            <a class="section" href={`/live?pid=${p.id}`}>
              <b>{nf.format(count(p.id, 'live'))}</b>
              <span>Live channels</span>
            </a>
            <a class="section" href={`/browse?pid=${p.id}&type=vod`}>
              <b>{nf.format(count(p.id, 'vod'))}</b>
              <span>Movies</span>
            </a>
            <a class="section" href={`/browse?pid=${p.id}&type=series`}>
              <b>{nf.format(count(p.id, 'series'))}</b>
              <span>Series</span>
            </a>
          </nav>

          <div class="actions">
            {#if p.source_type !== 'm3u_file'}
              <button class="act" onclick={() => refresh(p)} disabled={$syncState.running} aria-busy={isRefreshing}>
                <svg class:spin={isRefreshing} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 01-15.5 6.2L3 15.5"/><path d="M3 12a9 9 0 0115.5-6.2L21 8.5"/><polyline points="21 3 21 8.5 15.5 8.5"/><polyline points="3 21 3 15.5 8.5 15.5"/></svg>
                {isRefreshing ? `Updating ${Math.round(($syncState.progress ?? 0) * 100)}%` : 'Refresh'}
              </button>
            {/if}
            <button class="act" onclick={() => exportM3u(p)} disabled={busy === p.id}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 15V3"/><polyline points="7 8 12 3 17 8"/><path d="M5 14v5a2 2 0 002 2h10a2 2 0 002-2v-5"/></svg>
              Export M3U
            </button>
            {#if confirmDelete === p.id}
              <button class="act danger confirm" onclick={() => remove(p)}>Delete for good?</button>
            {:else}
              <button class="act danger" onclick={() => askDelete(p.id)} disabled={busy === p.id}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
                Delete
              </button>
            {/if}
          </div>

          {#if isRefreshing}
            <div class="progress" aria-hidden="true">
              <span style:width={`${Math.max(4, ($syncState.progress ?? 0) * 100)}%`}></span>
            </div>
            <p class="stage">{$syncState.stage}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if notice}
    <div class="toast" role="status" transition:fade={{ duration: 160 }}>
      <span>{notice.text}</span>
      {#if notice.path}
        <button onclick={() => showInFolder(notice!.path!)}>Show in folder</button>
      {/if}
    </div>
  {/if}
</div>

{#if showAdd}
  <AddPlaylistModal onclose={onAdded} />
{/if}

<style>
  .manager {
    --gutter: clamp(24px, 3.6vw, 60px);
    margin: -24px;
    min-height: 100vh;
    padding: 40px var(--gutter) 72px;
    background: radial-gradient(60% 40% at 90% 0%, oklch(0.4 0.15 27 / 0.18), transparent 70%);
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
    margin-bottom: 36px;
    max-width: 1280px;
  }
  .head h1 { font-size: clamp(2.25rem, 3.6vw, 3.25rem); font-weight: 900; letter-spacing: -0.035em; }
  .head p { margin-top: 6px; color: var(--color-text-muted); max-width: 60ch; }

  .btn-red {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 44px;
    padding: 0 20px 0 16px;
    border-radius: 6px;
    background: var(--color-accent);
    color: var(--color-on-accent);
    font-size: 0.9375rem;
    font-weight: 700;
    transition: background 150ms var(--ease-out);
  }
  .btn-red :global(svg) { width: 18px; height: 18px; }
  .btn-red:hover { background: var(--color-accent-hover); }
  .btn-red:focus-visible { outline: 2px solid var(--color-text); outline-offset: 3px; }
  .btn-red.big { height: 50px; padding: 0 28px; font-size: 1rem; }

  .list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 1280px;
  }

  .item {
    position: relative;
    display: grid;
    grid-template-columns: minmax(260px, 1.2fr) minmax(360px, 1.4fr) auto;
    align-items: center;
    gap: 28px;
    padding: 24px 26px;
    border-radius: 12px;
    background: oklch(0.2 0.005 25 / 0.85);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.06);
    overflow: hidden;
    transition: opacity 200ms var(--ease-out);
  }
  .item.dim { opacity: 0.55; }

  .identity { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .title-row { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .title-row h2 {
    font-size: 1.375rem;
    font-weight: 800;
    letter-spacing: -0.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .type {
    flex-shrink: 0;
    padding: 3px 8px;
    border-radius: 4px;
    background: oklch(1 0 0 / 0.08);
    color: var(--color-text-muted);
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
  .source {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 0.8125rem;
    color: var(--color-text-muted);
  }
  .sep { opacity: 0.5; }

  .account {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
    font-size: 0.8125rem;
    color: oklch(0.82 0.004 25);
  }
  .state { display: inline-flex; align-items: center; gap: 6px; font-weight: 700; }
  .state i { width: 7px; height: 7px; border-radius: 50%; background: var(--color-text-muted); }
  .state.ok { color: var(--color-accent-green); }
  .state.ok i { background: var(--color-accent-green); box-shadow: 0 0 0 3px oklch(0.76 0.13 165 / 0.2); }
  .warn { color: var(--color-accent-yellow); font-weight: 700; }
  .bad { color: var(--color-accent-soft); font-weight: 700; }

  .sections {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 16px;
    border-radius: 8px;
    background: oklch(1 0 0 / 0.04);
    color: var(--color-text);
    text-decoration: none;
    transition: background 150ms var(--ease-out), transform 150ms var(--ease-out);
  }
  .section b { font-size: 1.5rem; font-weight: 800; letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .section span { font-size: 0.8125rem; color: var(--color-text-muted); }
  .section:hover { background: oklch(1 0 0 / 0.09); transform: translateY(-2px); }
  .section:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .actions { display: flex; flex-direction: column; gap: 6px; min-width: 170px; }
  .act {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    height: 38px;
    padding: 0 14px;
    border-radius: 6px;
    background: oklch(1 0 0 / 0.06);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 600;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
    transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
  }
  .act :global(svg) { width: 16px; height: 16px; flex-shrink: 0; }
  .act:hover:not(:disabled) { background: oklch(1 0 0 / 0.12); }
  .act:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .act.danger:hover:not(:disabled) { color: var(--color-accent-soft); }
  .act.confirm { background: var(--color-accent); color: var(--color-on-accent); justify-content: center; }
  .act.confirm:hover { background: var(--color-accent-hover); color: var(--color-on-accent); }
  .spin { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: oklch(1 0 0 / 0.06);
  }
  .progress span { display: block; height: 100%; background: var(--color-accent); transition: width 300ms var(--ease-out); }
  .stage {
    grid-column: 1 / -1;
    margin-top: -16px;
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    padding: 80px 0;
    max-width: 620px;
  }
  .kicker { display: inline-flex; align-items: baseline; gap: 6px; font-size: 0.75rem; font-weight: 700; letter-spacing: 0.3em; }
  .kicker b { font-size: 1.5rem; font-weight: 900; letter-spacing: -0.04em; color: var(--color-accent); }
  .empty h2 { font-size: 2.5rem; font-weight: 900; letter-spacing: -0.03em; }
  .empty p { color: var(--color-text-muted); line-height: 1.6; }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 28px;
    z-index: 50;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 16px;
    max-width: min(760px, 90vw);
    padding: 12px 14px 12px 18px;
    border-radius: 8px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-size: 0.875rem;
    font-weight: 600;
    box-shadow: 0 20px 40px -12px oklch(0 0 0 / 0.7);
  }
  .toast span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .toast button {
    flex-shrink: 0;
    height: 30px;
    padding: 0 12px;
    border-radius: 5px;
    background: oklch(0.14 0.004 25);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
  }

  @media (max-width: 1150px) {
    .item { grid-template-columns: 1fr; gap: 18px; }
    .actions { flex-direction: row; flex-wrap: wrap; }
  }

  @media (prefers-reduced-motion: reduce) {
    .spin { animation: none; }
  }
</style>
