import { writable } from 'svelte/store';
import type { Playlist, ChannelGroup, ContentTypeCount, Channel, SeriesDetail, VodDetail } from '$lib/tauri';
import { getPlaylists, getPlaylistGroups, getContentTypeCounts } from '$lib/tauri';

export const playlists = writable<Playlist[]>([]);
export const selectedPlaylist = writable<Playlist | null>(null);
export const playlistGroups = writable<ChannelGroup[]>([]);
export const contentTypeCounts = writable<ContentTypeCount[]>([]);
export const playlistsLoading = writable(false);

// Per-tab state that persists when switching between live/vod/series
export interface TabState {
  tabGroups: ChannelGroup[];
  selectedGroup: string | null;
  groupChannels: Channel[];
  channelOffset: number;
  hasMore: boolean;
  viewingSeries: Channel | null;
  seriesDetail: SeriesDetail | null;
  selectedSeason: string | null;
  viewingVod: Channel | null;
  vodDetail: VodDetail | null;
}

// Browsing state that persists across navigation (leaving playlists page)
export interface PlaylistBrowsingState {
  activeTab: 'live' | 'vod' | 'series';
  searchQuery: string;
  searchResults: Channel[];
  tabCache: Record<string, TabState>;
}

export const browsingState = writable<PlaylistBrowsingState | null>(null);

export async function loadPlaylists() {
  playlistsLoading.set(true);
  try {
    const data = await getPlaylists();
    playlists.set(data);
  } catch (e) {
    console.error('Failed to load playlists:', e);
  } finally {
    playlistsLoading.set(false);
  }
}

export async function loadPlaylistGroups(playlistId: number) {
  try {
    const data = await getPlaylistGroups(playlistId);
    playlistGroups.set(data);
  } catch (e) {
    console.error('Failed to load playlist groups:', e);
  }
}

export async function loadContentTypeCounts(playlistId: number) {
  try {
    const data = await getContentTypeCounts(playlistId);
    contentTypeCounts.set(data);
  } catch (e) {
    console.error('Failed to load content type counts:', e);
  }
}
