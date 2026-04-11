import { writable } from 'svelte/store';
import type { Channel } from '$lib/tauri';
import { getChannels, searchChannels } from '$lib/tauri';

export const channels = writable<Channel[]>([]);
export const searchResults = writable<Channel[]>([]);
export const channelsLoading = writable(false);

export async function loadChannels(playlistId: number) {
  channelsLoading.set(true);
  try {
    const data = await getChannels(playlistId);
    channels.set(data);
  } catch (e) {
    console.error('Failed to load channels:', e);
  } finally {
    channelsLoading.set(false);
  }
}

export async function searchChannelsStore(query: string) {
  if (!query.trim()) {
    searchResults.set([]);
    return;
  }
  try {
    const data = await searchChannels(query);
    searchResults.set(data);
  } catch (e) {
    console.error('Failed to search channels:', e);
  }
}
