import { writable } from 'svelte/store';
import type { Channel } from '$lib/tauri';

export const currentChannel = writable<Channel | null>(null);
export const isPlaying = writable(false);
export const viewStartTime = writable<number | null>(null);
