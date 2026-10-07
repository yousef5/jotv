import { writable } from 'svelte/store';
import type { Playlist, Channel } from '$lib/tauri';
import { getPlaylists } from '$lib/tauri';

export const playlists = writable<Playlist[]>([]);

export async function loadPlaylists() {
  try {
    playlists.set(await getPlaylists());
  } catch (e) {
    console.error('Failed to load playlists:', e);
  }
}

// What a page should open when it mounts (set before navigating there)
export interface PendingOpen {
  playlistId: number;
  contentType: 'live' | 'vod' | 'series';
  /** Group to select; RECENTLY_ADDED_GROUP for the "Recently Added" list */
  group?: string;
  /** Item to open; omitted = just browse the group */
  channel?: Channel;
  /** detail = open the VOD/series page, play = start playback */
  action?: 'detail' | 'play';
}

export const RECENTLY_ADDED_GROUP = '__recently_added__';

export const pendingOpen = writable<PendingOpen | null>(null);
