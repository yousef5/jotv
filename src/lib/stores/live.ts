import { writable, get } from 'svelte/store';
import {
  mpvPlay, mpvLoad, mpvStop, mpvIsRunning, mpvStatus, mpvGoLive, mpvVolume, mpvToggleMute, mpvSeek, mpvSetTrack,
  recordViewing,
  embedAttach, embedPlace, getSetting, setSetting,
} from '$lib/tauri';
import type { Channel, MpvPlaybackState, MpvTrack } from '$lib/tauri';
import { guardTick, guardReset, flash } from './streamGuard';

// Live TV, movies and episodes all play in MPV, either rendered inside the app
// window (`--wid`) or in MPV's own window. Playback outlives any one page.

export type LiveMode = 'app' | 'window';
export type PlaybackKind = 'live' | 'vod';

export interface NowPlaying {
  /** Live channel, movie, or (for episodes) the series it belongs to */
  channel: Channel;
  kind: PlaybackKind;
  title: string;
  subtitle?: string;
  url: string;
  poster?: string | null;
  /** Setting key that stores the resume position (movies/episodes) */
  progressKey?: string;
  /** Where the mini player's "expand" goes */
  href: string;
  startedAt: number;
  /** true when MPV renders inside the app window */
  embedded: boolean;
}

/** A movie or episode to play in the app */
export interface MediaItem {
  channel: Channel;
  title: string;
  subtitle?: string;
  url: string;
  poster?: string | null;
  progressKey: string;
  href: string;
}

const MODE_KEY = 'jotv.liveMode';
const VOLUME_KEY = 'jotv.liveVolume';
const MUTED_KEY = 'jotv.liveMuted';
const ALANG_KEY = 'jotv.alang';
const SLANG_KEY = 'jotv.slang';

function readPref(key: string): string | undefined {
  try {
    return localStorage.getItem(key) || undefined;
  } catch {
    return undefined;
  }
}

function savedVolume(): number {
  try {
    const v = parseInt(localStorage.getItem(VOLUME_KEY) ?? '');
    return isNaN(v) ? 100 : Math.min(130, Math.max(0, v));
  } catch {
    return 100;
  }
}

function saveVolume(v: number) {
  try {
    localStorage.setItem(VOLUME_KEY, String(v));
  } catch {}
}

function savedMuted(): boolean {
  try {
    return localStorage.getItem(MUTED_KEY) === '1';
  } catch {
    return false;
  }
}

function saveMuted(m: boolean) {
  try {
    localStorage.setItem(MUTED_KEY, m ? '1' : '0');
  } catch {}
}

function savedMode(): LiveMode {
  try {
    return localStorage.getItem(MODE_KEY) === 'window' ? 'window' : 'app';
  } catch {
    return 'app';
  }
}

export const nowPlaying = writable<NowPlaying | null>(null);
export const liveStatus = writable<MpvPlaybackState | 'starting' | ''>('');
export const liveError = writable('');
/** Seconds playback trails the live edge (0 when at it) */
export const liveBehind = writable(0);
/** Behind by more than this counts as "not live" (matches the MPV badge) */
export const BEHIND_LIMIT = 10;
export const liveMode = writable<LiveMode>(savedMode());
/** In-app video fills the whole window */
export const liveFullscreen = writable(false);
/** An overlay (search, dialogs) is open: hide the native video surface, which would cover it */
export const overlayOpen = writable(false);
/** Element the in-app video should cover for that kind of playback; otherwise the corner mini player */
export const videoAnchor = writable<{ el: HTMLElement; kind: PlaybackKind } | null>(null);
/** Movies/episodes: position and length in seconds, and when the position was sampled */
export const playbackPos = writable({ pos: 0, duration: 0, at: 0, eof: false });
export const liveVolume = writable(savedVolume());
export const liveMuted = writable(savedMuted());

// MPV must start after its in-app surface is placed and visible: started on the
// hidden 1x1 placeholder it plays audio but never attaches the picture.
let surfaceWaiters: (() => void)[] = [];

/** Called by VideoSurface once the surface is visible at its real size. */
export function notifySurfacePlaced() {
  const waiters = surfaceWaiters;
  surfaceWaiters = [];
  waiters.forEach((resolve) => resolve());
}

function waitForSurface(timeoutMs = 800): Promise<void> {
  return new Promise((resolve) => {
    surfaceWaiters.push(resolve);
    setTimeout(resolve, timeoutMs);
  });
}

let pollTimer: ReturnType<typeof setInterval> | null = null;

function logViewing() {
  const np = get(nowPlaying);
  if (!np) return;
  const secs = Math.floor((Date.now() - np.startedAt) / 1000);
  // Library videos (negative ids) aren't channels
  if (secs > 5 && np.channel.id > 0) recordViewing(np.channel.id, secs).catch(() => {});
}

function stopPolling() {
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = null;
}

function startPolling() {
  stopPolling();
  let misses = 0;
  const poll = async () => {
    const np = get(nowPlaying);
    if (!np) return stopPolling();
    try {
      if (!(await mpvIsRunning())) {
        // MPV needs a moment to open its control socket after launch; only
        // give up after the grace period and two misses in a row
        if (Date.now() - np.startedAt < 10_000 || ++misses < 2) return;
        // MPV exited (window closed, or it crashed); make sure nothing keeps
        // playing where it can't be seen
        logViewing();
        nowPlaying.set(null);
        liveStatus.set('');
        liveFullscreen.set(false);
        mpvStop().catch(() => {});
        return stopPolling();
      }
      misses = 0;
      const status = await mpvStatus();
      liveStatus.set(status.state);
      if (np.kind === 'live') guardTick(status, { id: np.channel.id, url: np.url });
      playbackPos.set({ pos: status.time_pos, duration: status.duration, at: performance.now(), eof: status.eof });
      saveProgress(status.time_pos, status.duration, status.eof);
      liveBehind.set(Math.floor(status.behind_seconds));
      const v = Math.round(status.volume);
      if (v !== get(liveVolume)) {
        // Changed on the video itself (mouse wheel)
        liveVolume.set(v);
        saveVolume(v);
      }
      if (status.muted !== get(liveMuted)) {
        liveMuted.set(status.muted);
        saveMuted(status.muted);
      }
    } catch {
      liveStatus.set('offline');
    }
  };
  poll();
  pollTimer = setInterval(poll, 1500);
}

let lastSaved = 0;

/** Movies/episodes remember where you stopped (every ~5s, and at the end). */
function saveProgress(pos: number, duration: number, eof: boolean) {
  const np = get(nowPlaying);
  if (!np || np.kind !== 'vod' || !np.progressKey || duration <= 0) return;
  if (!eof && Math.abs(pos - lastSaved) < 5) return;
  lastSaved = pos;
  setSetting(np.progressKey, `${Math.floor(eof ? duration : pos)}|${Math.floor(duration)}`).catch(() => {});
}

/** Saved resume point, or 0 when unwatched / nearly finished. */
export async function resumePosition(progressKey: string): Promise<{ pos: number; duration: number }> {
  const raw = await getSetting(progressKey).catch(() => null);
  const [pos, duration] = (raw ?? '').split('|').map(Number);
  if (!pos || !duration || pos < 30 || pos > duration - 60) return { pos: 0, duration: duration || 0 };
  return { pos, duration };
}

async function start(np: Omit<NowPlaying, 'startedAt' | 'embedded'>, startAt?: number): Promise<void> {
  logViewing();

  let wid: number | undefined;
  if (get(liveMode) === 'app') {
    try {
      wid = await embedAttach();
    } catch (e) {
      liveError.set(`Playing in a separate window: ${e}`);
    }
  }

  nowPlaying.set({ ...np, startedAt: Date.now(), embedded: wid !== undefined });
  guardReset(np.kind === 'live' ? { id: np.channel.id, url: np.url } : undefined);
  liveStatus.set('starting');
  lastSaved = startAt ?? 0;
  playbackPos.set({ pos: startAt ?? 0, duration: 0, at: performance.now(), eof: false });
  if (wid !== undefined) {
    liveError.set('');
    await waitForSurface();
  }
  // Saved volume/mute apply when MPV starts; a running MPV keeps its own
  const opts = {
    wid,
    volume: get(liveVolume),
    muted: get(liveMuted),
    live: np.kind === 'live',
    start: startAt,
    alang: readPref(ALANG_KEY),
    slang: readPref(SLANG_KEY),
  };
  try {
    // A running MPV in the same mode just switches URL: instant channel change
    if (await mpvIsRunning()) await mpvLoad(np.url, np.title, opts);
    else await mpvPlay(np.url, np.title, opts);
    startPolling();
  } catch (e) {
    liveError.set(`MPV: ${e}`);
    liveStatus.set('');
    nowPlaying.set(null);
    stopPolling();
  }
}

export function playLive(channel: Channel): Promise<void> {
  return start({
    channel,
    kind: 'live',
    title: channel.name,
    subtitle: channel.group_name,
    url: channel.stream_url,
    poster: channel.logo_url,
    href: '/live',
  });
}

/** Plays a movie or episode in the app, resuming where it was left. */
export async function playMedia(item: MediaItem, opts: { fromStart?: boolean } = {}): Promise<void> {
  const resume = opts.fromStart ? 0 : (await resumePosition(item.progressKey)).pos;
  return start({ ...item, kind: 'vod' }, resume > 0 ? resume - 3 : undefined);
}

/** Switch audio/subtitle track and remember its language for next time. */
export async function setTrack(kind: 'audio' | 'sub', track: MpvTrack | null): Promise<void> {
  const lang = track?.lang ?? null;
  await mpvSetTrack(kind, track?.id ?? null, lang);
  try {
    const key = kind === 'audio' ? ALANG_KEY : SLANG_KEY;
    if (kind === 'sub' && !track) localStorage.setItem(key, 'no');
    else if (lang) localStorage.setItem(key, lang);
  } catch {}
}

export function seekBy(seconds: number): void {
  const p = get(playbackPos);
  playbackPos.set({ ...p, pos: Math.max(0, p.pos + seconds), at: performance.now() });
  mpvSeek(seconds).catch(() => {});
}

export function seekTo(seconds: number): void {
  const p = get(playbackPos);
  playbackPos.set({ ...p, pos: seconds, at: performance.now(), eof: false });
  mpvSeek(seconds, true).catch(() => {});
}

export function setVolume(v: number): void {
  const volume = Math.min(130, Math.max(0, Math.round(v)));
  liveVolume.set(volume);
  saveVolume(volume);
  if (get(liveMuted) && volume > 0) toggleMute();
  if (get(nowPlaying)) mpvVolume(volume).catch(() => {});
}

export function changeVolume(delta: number): void {
  setVolume(get(liveVolume) + delta);
}

export function toggleMute(): void {
  const muted = !get(liveMuted);
  liveMuted.set(muted);
  saveMuted(muted);
  if (get(nowPlaying)) mpvToggleMute().catch(() => {});
}

// ── Coming back to the app ──────────────────────────────────────────────────
// While the window is on another workspace or minimized, a live channel can
// fall behind (or freeze). On return, jump back to live, but only if it slipped
// while you were away; a channel you paused or rewound on purpose stays put.

const AWAY_SLIP_SECONDS = 10;
let awaySince: number | null = null;
let behindWhenLeft = 0;

function leftApp() {
  if (awaySince !== null) return;
  awaySince = Date.now();
  behindWhenLeft = get(liveBehind);
}

async function backInApp() {
  const since = awaySince;
  awaySince = null;
  const np = get(nowPlaying);
  if (since === null || !np || np.kind !== 'live' || Date.now() - since < 3000) return;
  const status = await mpvStatus().catch(() => null);
  if (!status || status.state === 'paused') return;
  if (status.behind_seconds - behindWhenLeft > AWAY_SLIP_SECONDS) {
    await goLive();
    flash('Caught up to live');
  }
}

if (typeof window !== 'undefined') {
  document.addEventListener('visibilitychange', () => (document.hidden ? leftApp() : backInApp()));
  window.addEventListener('blur', leftApp);
  window.addEventListener('focus', backInApp);
}

export async function goLive(): Promise<void> {
  await mpvGoLive().catch(() => {});
  liveBehind.set(0);
}

export async function stopLive(): Promise<void> {
  guardReset();
  logViewing();
  stopPolling();
  nowPlaying.set(null);
  liveStatus.set('');
  liveError.set('');
  liveFullscreen.set(false);
  await mpvStop().catch(() => {});
  await embedPlace(0, 0, 0, 0, false).catch(() => {});
}

/** Switches between in-app and separate-window playback, keeping the channel. */
export async function setLiveMode(mode: LiveMode): Promise<void> {
  liveMode.set(mode);
  try {
    localStorage.setItem(MODE_KEY, mode);
  } catch {}
  const np = get(nowPlaying);
  if (!np) return;
  if (mode === 'window') {
    liveFullscreen.set(false);
    await embedPlace(0, 0, 0, 0, false).catch(() => {});
  }
  const pos = get(playbackPos).pos;
  await start(np, np.kind === 'vod' && pos > 0 ? pos : undefined);
}
