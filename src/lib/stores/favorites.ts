import { writable } from 'svelte/store';
import type { FavoriteChannel } from '$lib/tauri';
import { getFavorites } from '$lib/tauri';

export const favorites = writable<FavoriteChannel[]>([]);
export const favoritesLoading = writable(false);

export async function loadFavorites() {
  favoritesLoading.set(true);
  try {
    const data = await getFavorites();
    favorites.set(data);
  } catch (e) {
    console.error('Failed to load favorites:', e);
  } finally {
    favoritesLoading.set(false);
  }
}
