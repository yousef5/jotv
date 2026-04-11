import { writable } from 'svelte/store';
import type { Channel } from '$lib/tauri';
import { getRecommendations } from '$lib/tauri';

export const recommendations = writable<Channel[]>([]);
export const recommendationsLoading = writable(false);

export async function loadRecommendations(limit?: number) {
  recommendationsLoading.set(true);
  try {
    const data = await getRecommendations(limit);
    recommendations.set(data);
  } catch (e) {
    console.error('Failed to load recommendations:', e);
  } finally {
    recommendationsLoading.set(false);
  }
}
