import { writable, derived } from 'svelte/store';
import { listen } from '@tauri-apps/api/event';
import type { Download } from '$lib/tauri';
import { getDownloads } from '$lib/tauri';

export const downloads = writable<Download[]>([]);
export const downloadsLoading = writable(false);
export const downloadStats = writable<Record<number, { speed_bps: number; eta_seconds: number | null }>>({});

// Derived stats for the header
export const downloadSummary = derived([downloads, downloadStats], ([$downloads, $stats]) => {
  const active = $downloads.filter(d => d.status === 'downloading');
  const completed = $downloads.filter(d => d.status === 'completed').length;
  const totalSpeed = active.reduce((sum, d) => sum + ($stats[d.id]?.speed_bps ?? 0), 0);
  return { activeCount: active.length, completed, total: $downloads.length, totalSpeed };
});

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

interface DownloadProgressPayload {
  id: number;
  progress: number;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bps: number;
  eta_seconds: number | null;
  status: string;
}

export async function setupDownloadListeners(): Promise<() => void> {
  const unlistenProgress = await listen<DownloadProgressPayload>('download-progress', (event) => {
    const p = event.payload;

    // Determine real status from progress value
    let status: string;
    if (p.progress >= 1.0) {
      status = 'completed';
    } else if (p.progress < 0) {
      status = p.status || 'failed';
    } else {
      status = 'downloading';
    }

    downloads.update((list) => {
      const exists = list.some(d => d.id === p.id);
      if (!exists) {
        // New download appeared — reload full list to get channel info
        loadDownloads();
        return list;
      }
      return list.map((d) =>
        d.id === p.id
          ? {
              ...d,
              progress: p.progress >= 0 ? p.progress : d.progress,
              downloaded_bytes: p.downloaded_bytes,
              total_bytes: p.total_bytes ?? d.total_bytes,
              status,
              ...(status === 'completed' ? { completed_at: new Date().toISOString() } : {}),
            }
          : d
      );
    });

    if (status === 'downloading') {
      downloadStats.update((s) => ({
        ...s,
        [p.id]: { speed_bps: p.speed_bps, eta_seconds: p.eta_seconds },
      }));
    } else {
      // Clear stats for completed/paused/failed
      downloadStats.update((s) => {
        const copy = { ...s };
        delete copy[p.id];
        return copy;
      });
    }
  });

  return () => {
    unlistenProgress();
  };
}
