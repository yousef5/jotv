import { writable, get } from 'svelte/store';
import { listen } from '@tauri-apps/api/event';
import { refreshPlaylist } from '$lib/tauri';
import type { Playlist } from '$lib/tauri';
import { playlists, loadPlaylists } from './playlists';

export type SyncPhase = 'connect' | 'download' | 'process' | 'save' | 'done';

export interface SyncSummary {
  added_live: number;
  added_vod: number;
  added_series: number;
  updated: number;
  removed: number;
}

export interface SyncState {
  running: boolean;
  /** 0..1 across all playlists being synced, null when indeterminate */
  progress: number | null;
  stage: string;
  phase: SyncPhase | null;
  /** Item counter for the current phase ("Saving 34,000 / 68,099") */
  current: number;
  total: number;
  /** Playlist being refreshed */
  playlistName: string;
  startedAt: number | null;
  error: string | null;
  lastSyncedAt: number | null;
  /** What the last finished sync changed */
  summary: SyncSummary | null;
  /** Bumped after every successful sync so views can reload their data */
  version: number;
}

export const syncState = writable<SyncState>({
  running: false,
  progress: null,
  stage: '',
  phase: null,
  current: 0,
  total: 0,
  playlistName: '',
  startedAt: null,
  error: null,
  lastSyncedAt: null,
  summary: null,
  version: 0,
});

// Share of a playlist's refresh each phase covers. Downloading the lists is
// most of the wait but reports no byte progress, so it's shown as a range.
const PHASE_SPAN: Record<SyncPhase, [number, number]> = {
  connect: [0, 0.03],
  download: [0.03, 0.6],
  process: [0.6, 0.7],
  save: [0.7, 0.99],
  done: [1, 1],
};

// Skip the launch sync for playlists refreshed this recently. Restarting the app
// (or the dev build) repeatedly would otherwise hammer the Xtream server, which
// IP-blocks bursts of requests.
const LAUNCH_MIN_AGE_MS = 15 * 60 * 1000;

let launchSyncStarted = false;

function isSyncable(p: Playlist): boolean {
  return p.source_type === 'xtream' || p.source_type === 'm3u_url';
}

function lastUpdatedMs(p: Playlist): number | null {
  if (!p.last_updated_at) return null;
  // SQLite datetime('now') is UTC without a zone marker
  const t = Date.parse(p.last_updated_at.replace(' ', 'T') + 'Z');
  return isNaN(t) ? null : t;
}

function newestUpdate(list: Playlist[]): number | null {
  const times = list.map(lastUpdatedMs).filter((t): t is number => t !== null);
  return times.length ? Math.max(...times) : null;
}

export async function syncPlaylists(targets?: Playlist[]): Promise<void> {
  if (get(syncState).running) return;

  if (!targets) {
    await loadPlaylists();
    targets = get(playlists).filter(isSyncable);
  }
  if (targets.length === 0) return;

  syncState.update((s) => ({
    ...s,
    running: true,
    progress: 0,
    stage: 'Connecting to the server…',
    phase: 'connect',
    current: 0,
    total: 0,
    playlistName: targets![0].name,
    startedAt: Date.now(),
    error: null,
    summary: null,
  }));

  let index = 0;
  const totals: SyncSummary = { added_live: 0, added_vod: 0, added_series: 0, updated: 0, removed: 0 };
  const unlisten = await listen<{ stage: string; current: number; total: number; phase?: SyncPhase }>(
    'xtream-import-progress',
    (event) => {
      const { stage, current, total, phase } = event.payload;
      syncState.update((s) => {
        const p = phase ?? s.phase ?? 'download';
        const [from, to] = PHASE_SPAN[p];
        const within = total > 0 ? Math.min(1, current / total) : 0;
        const playlistShare = from + (to - from) * within;
        return {
          ...s,
          stage,
          phase: p,
          current,
          total,
          progress: (index + playlistShare) / targets!.length,
        };
      });
    },
  );
  const unlistenSummary = await listen<SyncSummary>('library-sync-summary', (event) => {
    for (const k of Object.keys(totals) as (keyof SyncSummary)[]) totals[k] += event.payload[k] ?? 0;
  });

  let failed: string | null = null;
  try {
    for (index = 0; index < targets.length; index++) {
      syncState.update((s) => ({ ...s, playlistName: targets![index].name }));
      try {
        await refreshPlaylist(targets[index].id);
      } catch (e) {
        failed = `${targets[index].name}: ${e}`;
      }
    }
  } finally {
    unlisten();
    unlistenSummary();
  }

  await loadPlaylists();
  syncState.update((s) => ({
    ...s,
    running: false,
    progress: null,
    stage: '',
    phase: failed ? s.phase : 'done',
    error: failed,
    lastSyncedAt: failed ? s.lastSyncedAt : Date.now(),
    summary: failed ? null : { ...totals },
    version: s.version + 1,
  }));
}

/** Runs once per app session; refreshes playlists that are older than LAUNCH_MIN_AGE_MS. */
export async function syncOnLaunch(): Promise<void> {
  if (launchSyncStarted) return;
  launchSyncStarted = true;

  await loadPlaylists();
  const all = get(playlists).filter(isSyncable);
  syncState.update((s) => ({ ...s, lastSyncedAt: newestUpdate(all) }));

  const stale = all.filter((p) => {
    const t = lastUpdatedMs(p);
    return t === null || Date.now() - t > LAUNCH_MIN_AGE_MS;
  });
  if (stale.length) await syncPlaylists(stale);
}
