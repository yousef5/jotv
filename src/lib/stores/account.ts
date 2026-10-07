import { writable, derived, get } from 'svelte/store';
import { getXtreamAccountInfo } from '$lib/tauri';
import type { Playlist, XtreamAccountInfo } from '$lib/tauri';
import { playlists } from './playlists';

// Xtream account info (status, expiry, connections), shared by every page.
// It's an API call, so it's fetched once per session and refreshed only on demand.

export const accounts = writable<Record<number, XtreamAccountInfo | null>>({});

const fetchedAt = new Map<number, number>();
const MIN_REFRESH_MS = 60_000;

/** Loads one playlist's account info; `force` re-checks at most once a minute. */
export async function loadAccount(p: Playlist, force = false): Promise<XtreamAccountInfo | null> {
  if (p.source_type !== 'xtream' || !p.source_url || !p.xtream_username || !p.xtream_password) return null;
  const last = fetchedAt.get(p.id);
  if (last !== undefined && (!force || Date.now() - last < MIN_REFRESH_MS)) return get(accounts)[p.id] ?? null;
  fetchedAt.set(p.id, Date.now());
  let info: XtreamAccountInfo | null = null;
  try {
    info = await getXtreamAccountInfo(p.source_url, p.xtream_username, p.xtream_password);
  } catch {}
  accounts.update((a) => ({ ...a, [p.id]: info }));
  return info;
}

export function forgetAccount(id: number) {
  fetchedAt.delete(id);
  accounts.update((a) => {
    const { [id]: _, ...rest } = a;
    return rest;
  });
}

export function daysLeft(info: XtreamAccountInfo | null | undefined): number | null {
  const n = parseInt(info?.exp_date ?? '');
  return isNaN(n) ? null : Math.ceil((n - Date.now() / 1000) / 86400);
}

/** True when every allowed connection is in use (new streams get refused). */
export function connectionsFull(info: XtreamAccountInfo | null | undefined): boolean {
  const active = parseInt(String(info?.active_cons ?? ''));
  const max = parseInt(String(info?.max_connections ?? ''));
  return !isNaN(active) && !isNaN(max) && max > 0 && active >= max;
}

export interface AccountAlert {
  playlistId: number;
  name: string;
  kind: 'expiring' | 'expired';
  days: number;
}

/** Subscriptions that ended or end within a week. */
export const accountAlerts = derived([accounts, playlists], ([$accounts, $playlists]) => {
  const alerts: AccountAlert[] = [];
  for (const p of $playlists) {
    const days = daysLeft($accounts[p.id]);
    if (days === null) continue;
    if (days <= 0) alerts.push({ playlistId: p.id, name: p.name, kind: 'expired', days });
    else if (days <= 7) alerts.push({ playlistId: p.id, name: p.name, kind: 'expiring', days });
  }
  return alerts;
});
