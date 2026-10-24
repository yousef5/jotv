// Audio sync: some channels arrive with the sound ahead of (or behind) the
// picture in the source itself. The shift is remembered per channel or file.

/** Largest shift either way, in ms (MPV accepts up to 5 s) */
export const MAX_SYNC_MS = 3000;
export const SYNC_STEP_MS = 50;

/** Storage key: live channels by channel id, movies and episodes by their file */
export function syncKey(np: { kind: string; channel: { id: number }; progressKey?: string }): string {
  if (np.kind === 'live') return `jotv.audioSync.live.${np.channel.id}`;
  return `jotv.audioSync.${np.progressKey ?? `vod.${np.channel.id}`}`;
}

export function nudge(currentMs: number, deltaMs: number): number {
  return Math.max(-MAX_SYNC_MS, Math.min(MAX_SYNC_MS, currentMs + deltaMs));
}

/** Positive = the sound plays later */
export function syncLabel(ms: number): string {
  if (!ms) return 'In sync';
  const amount = Math.abs(ms) >= 1000 ? `${+(Math.abs(ms) / 1000).toFixed(2)} s` : `${Math.abs(ms)} ms`;
  return `Sound ${amount} ${ms > 0 ? 'later' : 'earlier'}`;
}
