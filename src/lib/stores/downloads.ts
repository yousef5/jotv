import { writable } from 'svelte/store';
import type { Download } from '$lib/tauri';
import { getDownloads } from '$lib/tauri';

export const downloads = writable<Download[]>([]);
export const downloadsLoading = writable(false);

export async function loadDownloads() {
  downloadsLoading.set(true);
  try {
    const data = await getDownloads();
    downloads.set(data);
  } catch (e) {
    console.error('Failed to load downloads:', e);
  } finally {
    downloadsLoading.set(false);
  }
}
