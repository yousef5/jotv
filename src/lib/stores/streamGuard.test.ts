import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';

vi.mock('$lib/tauri', () => ({
  mpvReconnect: vi.fn(async () => {}),
  mpvSetBuffer: vi.fn(async () => {}),
  mpvBadgeReconnecting: vi.fn(async () => {}),
  probeStream: vi.fn(async () => ({ ok: true, status: 200, message: 'OK' })),
}));

import { mpvReconnect, mpvSetBuffer, probeStream } from '$lib/tauri';
import { streamGuard, guardTick, guardReset, reloadStream } from './streamGuard';
import type { MpvStatus } from '$lib/tauri';

const URL_HLS = 'http://host/live/u/p/42.m3u8';
let channel = { id: 1, url: URL_HLS };
let pos = 100;

function status(state: MpvStatus['state'], advance = true): MpvStatus {
  if (advance && state === 'live') pos += 1.5;
  return { state, cache_seconds: 0, behind_seconds: 0, volume: 100, muted: false, time_pos: pos, duration: 0, eof: false };
}

/** Polls every 1.5s like the real player, for `ms` milliseconds. */
async function run(state: MpvStatus['state'], ms: number, advance = true) {
  for (let t = 0; t < ms; t += 1500) {
    guardTick(status(state, advance), channel);
    await vi.advanceTimersByTimeAsync(1500);
  }
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  vi.mocked(probeStream).mockResolvedValue({ ok: true, status: 200, message: 'OK' });
  channel = { id: channel.id + 1, url: URL_HLS };
  pos = 100;
  guardReset(channel);
});

afterEach(() => {
  guardReset();
  vi.useRealTimers();
});

describe('stream guard', () => {
  it('leaves a healthy stream alone', async () => {
    await run('live', 60_000);
    expect(get(streamGuard).phase).toBe('ok');
    expect(get(streamGuard).health).toBe('good');
    expect(mpvReconnect).not.toHaveBeenCalled();
  });

  it('reconnects when buffering stalls, then reports recovery', async () => {
    await run('live', 25_000);
    await run('buffering', 12_000);
    expect(get(streamGuard).phase).toBe('recovering');
    expect(mpvReconnect).toHaveBeenCalledWith(URL_HLS);

    await run('live', 4_500);
    expect(get(streamGuard).phase).toBe('ok');
    expect(get(streamGuard).notice).toBe('Reconnected');
  });

  it('is more patient while a channel is starting', async () => {
    await run('buffering', 15_000);
    expect(mpvReconnect).not.toHaveBeenCalled();
  });

  it('does not treat a paused stream as broken', async () => {
    await run('live', 25_000);
    await run('paused', 60_000);
    expect(get(streamGuard).phase).toBe('ok');
    expect(mpvReconnect).not.toHaveBeenCalled();
  });

  it('catches a frozen picture', async () => {
    await run('live', 25_000);
    await run('live', 13_000, false);
    expect(get(streamGuard).phase).toBe('recovering');
    expect(get(streamGuard).reason).toBe('The picture froze');
  });

  it('backs off and stops when the server refuses connections', async () => {
    vi.mocked(probeStream).mockResolvedValue({ ok: false, status: 463, message: 'HTTP 463' });
    await run('live', 25_000);
    await run('nosignal', 8_000); // attempt 1: immediate reconnect, no probe
    expect(mpvReconnect).toHaveBeenCalledTimes(1);
    await run('nosignal', 20_000); // attempt 1 times out → attempt 2 probes first
    expect(probeStream).toHaveBeenCalled();
    expect(get(streamGuard).phase).toBe('blocked');
    expect(mpvReconnect).toHaveBeenCalledTimes(1); // no reconnect into the block
  });

  it('tries the other container on the third attempt', async () => {
    await run('live', 25_000);
    await run('nosignal', 8_000);
    await run('nosignal', 20_000); // attempt 2 after 3s backoff + timeout
    await run('nosignal', 30_000); // attempt 3 after 8s backoff
    const urls = vi.mocked(mpvReconnect).mock.calls.map((c) => c[0]);
    expect(urls).toContain('http://host/live/u/p/42.ts');
  });

  it('gives up after four tries in five minutes', async () => {
    await run('live', 25_000);
    await run('nosignal', 150_000);
    expect(get(streamGuard).phase).toBe('failed');
    expect(vi.mocked(mpvReconnect).mock.calls.length).toBeLessThanOrEqual(4);
  });

  it('manual reload after giving up starts fresh', async () => {
    await run('live', 25_000);
    await run('nosignal', 150_000);
    vi.mocked(mpvReconnect).mockClear();
    reloadStream();
    await vi.advanceTimersByTimeAsync(0);
    expect(mpvReconnect).toHaveBeenCalledTimes(1);
    expect(get(streamGuard).phase).toBe('recovering');
  });

  it('switches to stable buffering after repeated stalls', async () => {
    await run('live', 25_000);
    for (let i = 0; i < 3; i++) {
      await run('buffering', 3_000);
      await run('live', 6_000);
    }
    expect(get(streamGuard).stable).toBe(true);
    expect(get(streamGuard).stableAuto).toBe(true);
    expect(mpvSetBuffer).toHaveBeenCalledWith('stable');
  });
});
