import { writable, get } from 'svelte/store';
import { mpvReconnect, mpvSetBuffer, mpvBadgeReconnecting, probeStream } from '$lib/tauri';
import type { MpvStatus } from '$lib/tauri';

// Keeps a weak live stream going: notices stalls, drops and frozen pictures,
// reconnects with growing waits, and backs off when the server is refusing us.
// Xtream servers IP-block clients that retry too fast (HTTP 461/463), so every
// retry is paced, capped, and checked with a single probe before it hits MPV.

export type GuardPhase = 'ok' | 'recovering' | 'blocked' | 'failed';
export type Health = 'good' | 'fair' | 'poor';

export interface GuardState {
  phase: GuardPhase;
  attempt: number;
  maxAttempts: number;
  /** Why the guard stepped in, in plain words */
  reason: string;
  /** When the next retry fires (for countdowns) */
  nextRetryAt: number | null;
  health: Health;
  /** Buffering stalls in the last two minutes */
  stalls: number;
  /** Bigger buffer before resuming after a stall */
  stable: boolean;
  /** Stable mode was switched on by the guard, not the user */
  stableAuto: boolean;
  /** Short message after something happened ("Reconnected") */
  notice: string;
}

const MAX_ATTEMPTS = 4;
const ATTEMPT_WINDOW_MS = 5 * 60_000;
const BACKOFF_MS = [0, 3_000, 8_000, 20_000];
const BLOCKED_WAIT_MS = 90_000;
const STALL_MS = 10_000;
const START_STALL_MS = 18_000;
const DROP_MS = 6_000;
const FROZEN_MS = 10_000;
const ATTEMPT_TIMEOUT_MS = 15_000;
const STALL_WINDOW_MS = 120_000;
const STALLS_FOR_STABLE = 3;
const STABLE_KEY = 'jotv.liveStable';

function savedStable(): boolean {
  try {
    return localStorage.getItem(STABLE_KEY) === '1';
  } catch {
    return false;
  }
}

const initial = (): GuardState => ({
  phase: 'ok',
  attempt: 0,
  maxAttempts: MAX_ATTEMPTS,
  reason: '',
  nextRetryAt: null,
  health: 'good',
  stalls: 0,
  stable: savedStable(),
  stableAuto: false,
  notice: '',
});

export const streamGuard = writable<GuardState>(initial());

// ── Per-channel tracking ─────────────────────────────────────────────────────
let channelId: number | null = null;
let url = '';
let startedAt = 0;
let lastState = '';
let bufferingSince: number | null = null;
let dropSince: number | null = null;
let lastPos = -1;
let posStuckSince: number | null = null;
let stallTimes: number[] = [];
let attemptTimes: number[] = [];
let attemptSentAt: number | null = null;
/** Buffer mode to push once MPV is up for this channel */
let pendingBuffer = false;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
let noticeTimer: ReturnType<typeof setTimeout> | undefined;

function set(patch: Partial<GuardState>) {
  streamGuard.update((g) => ({ ...g, ...patch }));
}

/** Short message on the player ("Reconnected", "Caught up to live"…) */
export function flash(notice: string) {
  set({ notice });
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => set({ notice: '' }), 5000);
}

function clearTracking() {
  bufferingSince = null;
  dropSince = null;
  posStuckSince = null;
  lastPos = -1;
}

/** New channel (or stopped): forget everything about the previous stream. */
export function guardReset(next?: { id: number; url: string }) {
  clearTimeout(retryTimer);
  channelId = next?.id ?? null;
  url = next?.url ?? '';
  startedAt = Date.now();
  lastState = '';
  stallTimes = [];
  attemptTimes = [];
  attemptSentAt = null;
  clearTracking();
  mpvBadgeReconnecting(false).catch(() => {});
  const stable = get(streamGuard).stable && !get(streamGuard).stableAuto;
  streamGuard.set({ ...initial(), stable });
  pendingBuffer = !!next;
}

/** Same channel in the other container: some servers serve one far better. */
function alternate(u: string): string {
  if (u.includes('.m3u8')) return u.replace('.m3u8', '.ts');
  if (/\.ts(\?|$)/.test(u)) return u.replace(/\.ts(\?|$)/, '.m3u8$1');
  return u;
}

function healthFrom(stalls: number, phase: GuardPhase): Health {
  if (phase !== 'ok') return 'poor';
  return stalls === 0 ? 'good' : stalls < STALLS_FOR_STABLE ? 'fair' : 'poor';
}

function recover(reason: string, now = Date.now()) {
  attemptTimes = attemptTimes.filter((t) => now - t < ATTEMPT_WINDOW_MS);
  const n = attemptTimes.length;
  if (n >= MAX_ATTEMPTS) {
    clearTimeout(retryTimer);
    mpvBadgeReconnecting(false).catch(() => {});
    set({ phase: 'failed', reason, nextRetryAt: null, health: 'poor' });
    return;
  }
  const delay = BACKOFF_MS[Math.min(n, BACKOFF_MS.length - 1)];
  set({ phase: 'recovering', attempt: n + 1, reason, nextRetryAt: now + delay, health: 'poor' });
  mpvBadgeReconnecting(true).catch(() => {});
  clearTimeout(retryTimer);
  retryTimer = setTimeout(() => attempt(n), delay);
}

async function attempt(n: number) {
  const id = channelId;
  if (id === null) return;
  attemptTimes.push(Date.now());

  // From the second try on, one cheap check first: if the server is refusing
  // us, reconnecting would only extend the block
  if (n >= 1) {
    const probe = await probeStream(url).catch(() => null);
    if (channelId !== id) return;
    const code = probe?.status ?? 0;
    if (probe && !probe.ok && code >= 400 && code < 500) {
      const wait = Date.now() + BLOCKED_WAIT_MS;
      set({
        phase: 'blocked',
        reason: `The server is refusing new connections (HTTP ${code}). Too many devices on this account, or a temporary block.`,
        nextRetryAt: wait,
      });
      clearTimeout(retryTimer);
      retryTimer = setTimeout(() => recover('Trying again after the server pause'), BLOCKED_WAIT_MS);
      return;
    }
  }

  const target = n === 2 ? alternate(url) : url;
  clearTracking();
  attemptSentAt = Date.now();
  await mpvReconnect(target).catch(() => {});
}

/** Feed every MPV status poll for a live channel. */
export function guardTick(status: MpvStatus, live: { id: number; url: string }) {
  const now = Date.now();
  if (live.id !== channelId) guardReset(live);
  const g = get(streamGuard);
  const state = status.state;
  const pos = status.time_pos;

  if (pendingBuffer) {
    pendingBuffer = false;
    mpvSetBuffer(g.stable ? 'stable' : 'fast').catch(() => {});
  }

  // Stall history drives health and smart buffering
  if (state === 'buffering' && lastState !== 'buffering' && now - startedAt > 5_000) stallTimes.push(now);
  stallTimes = stallTimes.filter((t) => now - t < STALL_WINDOW_MS);
  lastState = state;

  if (g.phase === 'recovering' && attemptSentAt !== null) {
    if (state === 'live' && now - attemptSentAt > 2_000) {
      // Playing again
      attemptSentAt = null;
      mpvBadgeReconnecting(false).catch(() => {});
      set({ phase: 'ok', reason: '', nextRetryAt: null, health: healthFrom(stallTimes.length, 'ok'), stalls: stallTimes.length });
      flash('Reconnected');
      return;
    }
    if (now - attemptSentAt > ATTEMPT_TIMEOUT_MS) {
      attemptSentAt = null;
      recover(g.reason || 'The stream is still not playing', now);
    }
    return;
  }
  if (g.phase !== 'ok') return;

  // Several stalls in a short time: wait for a bigger buffer before resuming
  if (stallTimes.length >= STALLS_FOR_STABLE && !g.stable) {
    mpvSetBuffer('stable').catch(() => {});
    set({ stable: true, stableAuto: true });
    flash('Stable mode on: a slightly bigger buffer to stop the cuts');
  }
  set({ stalls: stallTimes.length, health: healthFrom(stallTimes.length, 'ok') });

  if (state === 'paused') {
    clearTracking();
    return;
  }

  // Stuck buffering
  if (state === 'buffering') {
    bufferingSince ??= now;
    const limit = now - startedAt < 20_000 ? START_STALL_MS : STALL_MS;
    if (now - bufferingSince > limit) return recover('The stream stalled and stopped loading', now);
  } else {
    bufferingSince = null;
  }

  // Dropped: nothing decoding and nothing loading
  if (state === 'nosignal' || state === 'offline') {
    dropSince ??= now;
    if (now - dropSince > DROP_MS) return recover('The stream dropped', now);
  } else {
    dropSince = null;
  }

  // Frozen picture: "playing" but the clock doesn't move
  if (state === 'live') {
    if (pos > lastPos + 0.05) {
      lastPos = pos;
      posStuckSince = null;
    } else {
      posStuckSince ??= now;
      if (now - posStuckSince > FROZEN_MS) return recover('The picture froze', now);
    }
  }
}

/** Manual reload (↻ / R): reconnect now, and give a failed channel a fresh set of tries. */
export function reloadStream() {
  if (channelId === null) return;
  const g = get(streamGuard);
  if (g.phase === 'failed' || g.phase === 'blocked') attemptTimes = [];
  clearTimeout(retryTimer);
  mpvBadgeReconnecting(true).catch(() => {});
  set({ phase: 'recovering', attempt: 1, reason: 'Reloading the stream', nextRetryAt: Date.now() });
  attempt(0);
}

export function setStable(on: boolean) {
  set({ stable: on, stableAuto: false });
  try { localStorage.setItem(STABLE_KEY, on ? '1' : '0'); } catch {}
  if (channelId !== null) mpvSetBuffer(on ? 'stable' : 'fast').catch(() => {});
}
