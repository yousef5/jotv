import { writable } from 'svelte/store';
import type { Playlist, ChannelGroup, ContentTypeCount } from '$lib/tauri';
import { getPlaylists, getPlaylistGroups, getContentTypeCounts } from '$lib/tauri';

export const playlists = writable<Playlist[]>([]);
export const selectedPlaylist = writable<Playlist | null>(null);
export const playlistGroups = writable<ChannelGroup[]>([]);
export const contentTypeCounts = writable<ContentTypeCount[]>([]);
export const playlistsLoading = writable(false);

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
