import { writable } from 'svelte/store';

/** Global search palette: open state and an optional starting query */
export const searchPalette = writable<{ open: boolean; query: string }>({ open: false, query: '' });

export function openSearch(query = '') {
  searchPalette.set({ open: true, query });
}

export function closeSearch() {
  searchPalette.update((s) => ({ ...s, open: false }));
}
