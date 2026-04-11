import { writable } from 'svelte/store';
import { listen } from '@tauri-apps/api/event';
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

interface DownloadProgressPayload {
  id: number;
  progress: number;
  downloaded_bytes: number;
  total_bytes: number | null;
}

interface DownloadCompletePayload {
  id: number;
  file_path: string | null;
}

interface DownloadFailedPayload {
  id: number;
  error: string;
}

export async function setupDownloadListeners(): Promise<() => void> {
  const unlistenProgress = await listen<DownloadProgressPayload>('download-progress', (event) => {
    downloads.update((list) =>
      list.map((d) =>
        d.id === event.payload.id
          ? {
              ...d,
              progress: event.payload.progress,
              downloaded_bytes: event.payload.downloaded_bytes,
              total_bytes: event.payload.total_bytes ?? d.total_bytes,
              status: 'downloading',
            }
          : d
      )
    );
  });

  const unlistenComplete = await listen<DownloadCompletePayload>('download-complete', (event) => {
    downloads.update((list) =>
      list.map((d) =>
        d.id === event.payload.id
          ? {
              ...d,
              status: 'completed',
              progress: 100,
              file_path: event.payload.file_path ?? d.file_path,
              completed_at: new Date().toISOString(),
            }
          : d
      )
    );
  });

  const unlistenFailed = await listen<DownloadFailedPayload>('download-failed', (event) => {
    downloads.update((list) =>
      list.map((d) =>
        d.id === event.payload.id
          ? {
              ...d,
              status: 'failed',
            }
          : d
      )
    );
  });

  return () => {
    unlistenProgress();
    unlistenComplete();
    unlistenFailed();
  };
}
