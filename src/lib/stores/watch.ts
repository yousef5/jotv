import { writable } from 'svelte/store';
import type { Channel } from '$lib/tauri';

/** One playable movie or episode in the player's queue */
export interface WatchItem {
  /** Episode id, or the movie's channel id */
  id: number;
  url: string;
  title: string;
  subtitle?: string;
  progressKey: string;
  season?: string;
  episode?: string;
}

/** What the title page hands to /watch, so it doesn't refetch series info */
export interface WatchRequest {
  channel: Channel;
  poster?: string | null;
  items: WatchItem[];
  index: number;
  fromStart?: boolean;
}

export const pendingWatch = writable<WatchRequest | null>(null);

export function watchHref(channelId: number, item?: WatchItem, isSeries = false): string {
  return isSeries && item ? `/watch?id=${channelId}&ep=${item.id}` : `/watch?id=${channelId}`;
}
