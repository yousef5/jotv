import { writable } from 'svelte/store';
import type { EpgEntry } from '$lib/tauri';
import { getEpgForChannel, getCurrentProgram } from '$lib/tauri';

export const channelEpg = writable<EpgEntry[]>([]);
export const currentProgram = writable<EpgEntry | null>(null);
export const epgLoading = writable(false);

export async function loadEpgForChannel(epgId: string) {
  epgLoading.set(true);
  try {
    const data = await getEpgForChannel(epgId);
    channelEpg.set(data);
  } catch (e) {
    console.error('Failed to load EPG:', e);
  } finally {
    epgLoading.set(false);
  }
}

export async function loadCurrentProgram(epgId: string) {
  try {
    const data = await getCurrentProgram(epgId);
    currentProgram.set(data);
  } catch (e) {
    console.error('Failed to load current program:', e);
  }
}
