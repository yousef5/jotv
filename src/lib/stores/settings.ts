import { writable } from 'svelte/store';
import { getSetting, setSetting, getAllSettings } from '$lib/tauri';

export const theme = writable<string>('dark');
export const externalPlayer = writable<string>('');
export const externalPlayerPath = writable<string>('');
export const downloadDir = writable<string>('');
export const settingsLoading = writable(false);

export async function loadSettings() {
  settingsLoading.set(true);
  try {
    const all = await getAllSettings();
    const map = new Map(all);

    theme.set(map.get('theme') ?? 'dark');
    externalPlayer.set(map.get('external_player') ?? '');
    externalPlayerPath.set(map.get('external_player_path') ?? '');
    downloadDir.set(map.get('download_dir') ?? '');
  } catch (e) {
    console.error('Failed to load settings:', e);
  } finally {
    settingsLoading.set(false);
  }
}

export async function updateSetting(key: string, value: string) {
  try {
    await setSetting(key, value);
    // Update local store
    switch (key) {
      case 'theme':
        theme.set(value);
        break;
      case 'external_player':
        externalPlayer.set(value);
        break;
      case 'external_player_path':
        externalPlayerPath.set(value);
        break;
      case 'download_dir':
        downloadDir.set(value);
        break;
    }
  } catch (e) {
    console.error('Failed to update setting:', e);
  }
}
