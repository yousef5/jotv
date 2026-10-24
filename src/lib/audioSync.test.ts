import { describe, it, expect } from 'vitest';
import { syncKey, nudge, syncLabel, MAX_SYNC_MS } from './audioSync';

describe('audio sync', () => {
  it('remembers live channels by channel and movies/episodes by their file', () => {
    expect(syncKey({ kind: 'live', channel: { id: 42 } })).toBe('jotv.audioSync.live.42');
    expect(syncKey({ kind: 'vod', channel: { id: 7 }, progressKey: 'progress_ep_99' })).toBe('jotv.audioSync.progress_ep_99');
    expect(syncKey({ kind: 'vod', channel: { id: 7 } })).toBe('jotv.audioSync.vod.7');
  });

  it('moves in steps and stops at the limit', () => {
    expect(nudge(0, 50)).toBe(50);
    expect(nudge(-50, 50)).toBe(0);
    expect(nudge(MAX_SYNC_MS - 20, 50)).toBe(MAX_SYNC_MS);
    expect(nudge(-MAX_SYNC_MS, -50)).toBe(-MAX_SYNC_MS);
  });

  it('says which way the sound moved', () => {
    expect(syncLabel(0)).toBe('In sync');
    expect(syncLabel(150)).toBe('Sound 150 ms later');
    expect(syncLabel(-100)).toBe('Sound 100 ms earlier');
    expect(syncLabel(1250)).toBe('Sound 1.25 s later');
  });
});
