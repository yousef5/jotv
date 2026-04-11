import { writable } from 'svelte/store';
import type { Channel } from '$lib/tauri';
import { getRecentlyWatched } from '$lib/tauri';

export const recentlyWatched = writable<Channel[]>([]);
export const historyLoading = writable(false);

export async function loadRecentlyWatched(limit?: number) {
  historyLoading.set(true);
  try {
    const data = await getRecentlyWatched(limit);
    recentlyWatched.set(data);
  } catch (e) {
    console.error('Failed to load recently watched:', e);
  } finally {
    historyLoading.set(false);
  }
}
