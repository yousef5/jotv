import { writable, get } from 'svelte/store';
import { convertFileSrc } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  reelsList, reelCategories, reelProbe, startSocialDownload, getSetting, getDefaultDownloadDir,
} from '$lib/tauri';
import type { Reel, ReelCategory, Channel, SocialVideoInfo, SocialDownloadProgress } from '$lib/tauri';

// The video library: finished social downloads plus imported local files,
// sorted into main categories → sub-categories and tagged.

export const reels = writable<Reel[]>([]);
export const reelCats = writable<ReelCategory[]>([]);
export const reelsLoaded = writable(false);

/** What the reel player plays through (the list the user opened it from) */
export const reelQueue = writable<string[]>([]);

export async function loadReels(): Promise<void> {
  const [list, cats] = await Promise.all([reelsList(), reelCategories()]);
  reels.set(list);
  reelCats.set(cats);
  reelsLoaded.set(true);
  probeMissing();
}

export function patchReel(r: Reel) {
  reels.update((list) => list.map((x) => (x.id === r.id ? r : x)));
}

// ── Thumbnails & durations (ffmpeg, a couple at a time) ─────────────────────
const probing = new Set<string>();
let active = 0;

function probeMissing() {
  // Audio-only files have a duration but no picture: nothing more to find
  const todo = get(reels).filter(
    (r) =>
      !probing.has(r.id) &&
      (r.kind === 'image' ? !r.thumb_path || r.width === null : r.duration === null || (r.width !== null && !r.thumb_path)),
  );
  todo.forEach((r) => probing.add(r.id));
  const queue = [...todo];
  const next = async () => {
    const r = queue.shift();
    if (!r) return;
    active++;
    try {
      patchReel(await reelProbe(r.id));
    } catch {
      // Unreadable file: keep the remote thumbnail
    } finally {
      active--;
      next();
    }
  };
  for (let i = active; i < 2; i++) next();
}

export function thumbSrc(r: Reel): string | null {
  if (r.thumb_path) {
    try {
      return convertFileSrc(r.thumb_path);
    } catch {
      // Outside Tauri
    }
  }
  return r.thumbnail;
}

/** Probed, and there's no video stream (an m4a, or an audio-only webm) */
export function isAudio(r: Reel): boolean {
  return r.kind !== 'image' && r.duration !== null && !r.width && !r.thumb_path;
}

/** The full picture of a photo (served from the app's own copy) */
export function fileSrc(r: Reel): string {
  try {
    return convertFileSrc(r.file_path);
  } catch {
    return r.file_path;
  }
}

/** Width ÷ height; reels default to portrait, everything else to 16:9. */
export function aspect(r: Reel): number {
  if (r.width && r.height) return Math.min(2.4, Math.max(0.5, r.width / r.height));
  return /instagram|tiktok/i.test(r.platform) ? 9 / 16 : 16 / 9;
}

export function isPortrait(r: Reel): boolean {
  return aspect(r) < 0.9;
}

// ── Categories ──────────────────────────────────────────────────────────────
export function childrenOf(cats: ReelCategory[], id: number): ReelCategory[] {
  return cats.filter((c) => c.parent_id === id);
}

/** A category plus its sub-categories */
export function familyIds(cats: ReelCategory[], id: number): Set<number> {
  return new Set([id, ...childrenOf(cats, id).map((c) => c.id)]);
}

export function categoryPath(cats: ReelCategory[], id: number | null): ReelCategory[] {
  const c = cats.find((x) => x.id === id);
  if (!c) return [];
  const parent = c.parent_id !== null ? cats.find((x) => x.id === c.parent_id) : undefined;
  return parent ? [parent, c] : [c];
}

/** Category colours: picked for a dark UI, readable as dots and tints */
export const CATEGORY_COLORS = [
  'oklch(0.68 0.19 25)',
  'oklch(0.75 0.15 60)',
  'oklch(0.82 0.15 95)',
  'oklch(0.74 0.16 150)',
  'oklch(0.74 0.12 200)',
  'oklch(0.68 0.15 255)',
  'oklch(0.66 0.18 300)',
  'oklch(0.7 0.17 345)',
];

export function categoryColor(cats: ReelCategory[], id: number | null): string {
  const path = categoryPath(cats, id);
  const main = path[0];
  if (!main) return 'oklch(0.6 0.01 25)';
  return main.color ?? CATEGORY_COLORS[main.id % CATEGORY_COLORS.length];
}

/** Menu entries: Uncategorized, then each main category with its subs under it */
export function categoryOptions(cats: ReelCategory[]): { value: string; label: string; sub?: boolean; color?: string }[] {
  const out: { value: string; label: string; sub?: boolean; color?: string }[] = [{ value: '', label: 'Uncategorized' }];
  for (const m of cats.filter((c) => c.parent_id === null)) {
    out.push({ value: String(m.id), label: m.name, color: categoryColor(cats, m.id) });
    for (const s of childrenOf(cats, m.id)) out.push({ value: String(s.id), label: s.name, sub: true });
  }
  return out;
}

/** "#hashtags" in a title, as tags ("#Real_Madrid" → "Real Madrid") */
export function titleHashtags(title: string): string[] {
  const out: string[] = [];
  for (const m of title.matchAll(/#([\p{L}\p{N}_]{2,40})/gu)) {
    const t = m[1].replace(/_/g, ' ');
    if (!out.some((x) => x.toLowerCase() === t.toLowerCase())) out.push(t);
  }
  return out;
}

/**
 * A readable name from a site's title: drops Facebook's "747K views · 6.4K
 * reactions |" prefix, the "| Channel" suffix and the #hashtags (they become tags).
 */
export function cleanTitle(title: string, uploader?: string | null): string {
  let t = title
    .replace(/^\s*[\d.,]+\s*[KMB]?\s*(views?|plays?)\b.*?\|\s*/i, '')
    .replace(/^\s*[\d.,]+\s*[KMB]?\s*(reactions?|likes?)\b.*?\|\s*/i, '');
  if (uploader) {
    const esc = uploader.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    t = t.replace(new RegExp(`\\s*[|\\-–—]\\s*${esc}\\s*$`, 'i'), '');
  }
  t = t.replace(/#[\p{L}\p{N}_]{2,40}/gu, '').replace(/\s{2,}/g, ' ').trim().replace(/[\s:|\-–—]+$/, '').trim();
  return t || title.trim();
}

// ── Formatting ──────────────────────────────────────────────────────────────
export function fmtDuration(secs: number | null): string {
  if (!secs || !isFinite(secs)) return '';
  const s = Math.round(secs);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

export function fmtSize(bytes: number | null): string {
  if (!bytes) return '';
  const units = ['B', 'KB', 'MB', 'GB'];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 10 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

export const PLATFORM_COLORS: Record<string, string> = {
  YouTube: 'oklch(0.63 0.25 29)',
  Instagram: 'oklch(0.63 0.24 350)',
  Facebook: 'oklch(0.6 0.19 260)',
  TikTok: 'oklch(0.85 0.13 195)',
  'Twitter/X': 'oklch(0.7 0.14 240)',
  Reddit: 'oklch(0.68 0.21 40)',
  Twitch: 'oklch(0.58 0.24 295)',
  Local: 'oklch(0.75 0.02 25)',
};

/** Plays a library video through the shared MPV pipeline. */
export function reelChannel(r: Reel): Channel {
  return {
    id: -1,
    playlist_id: -1,
    name: r.title,
    group_name: 'Library',
    stream_url: r.file_path,
    logo_url: thumbSrc(r),
    epg_id: null,
    content_type: 'vod',
    created_at: r.created_at,
  };
}

// ── Adding from a link (yt-dlp) ─────────────────────────────────────────────

export interface LinkDownload {
  id: string;
  title: string;
  thumbnail: string | null;
  platform: string;
  progress: number;
  speed: string | null;
  eta: string | null;
  status: 'downloading' | 'failed';
  error?: string;
  /** To try again */
  request: { url: string; format: string; label: string | null; categoryId: number | null; tags: string[] };
}

/** Downloads started from the library, until they land in it */
export const linkDownloads = writable<LinkDownload[]>([]);

let listening = false;
async function listenDownloads() {
  if (listening) return;
  listening = true;
  try {
    await listen<SocialDownloadProgress>('social-download-progress', (e) => {
      const p = e.payload;
      const mine = get(linkDownloads).some((d) => d.id === p.download_id);
      if (!mine) return;
      if (p.status === 'completed') {
        linkDownloads.update((l) => l.filter((d) => d.id !== p.download_id));
        loadReels().catch(() => {});
      } else if (p.status.startsWith('failed') || p.status === 'cancelled') {
        linkDownloads.update((l) =>
          l.map((d) => (d.id === p.download_id ? { ...d, status: 'failed', error: p.status.replace(/^failed:\s*/, '') } : d)),
        );
      } else {
        linkDownloads.update((l) =>
          l.map((d) => (d.id === p.download_id ? { ...d, progress: Math.max(0, p.progress), speed: p.speed, eta: p.eta } : d)),
        );
      }
    });
  } catch {
    listening = false;
  }
}

async function downloadDir(): Promise<string> {
  return (await getSetting('social_download_dir').catch(() => null)) || (await getDefaultDownloadDir());
}

/** Downloads a video into the library, straight into a category with tags. */
export async function addFromLink(
  info: Pick<SocialVideoInfo, 'url' | 'title' | 'thumbnail' | 'platform'>,
  request: LinkDownload['request'],
): Promise<void> {
  await listenDownloads();
  const id = `social-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  linkDownloads.update((l) => [
    { id, title: info.title, thumbnail: info.thumbnail, platform: info.platform, progress: 0, speed: null, eta: null, status: 'downloading', request },
    ...l,
  ]);
  try {
    await startSocialDownload(request.url, request.format, await downloadDir(), id, info.title, info.thumbnail, info.platform, request.label, request.categoryId, request.tags);
  } catch (e) {
    linkDownloads.update((l) => l.map((d) => (d.id === id ? { ...d, status: 'failed', error: String(e) } : d)));
  }
}

export function retryLink(d: LinkDownload) {
  linkDownloads.update((l) => l.filter((x) => x.id !== d.id));
  return addFromLink({ ...d, url: d.request.url }, d.request);
}

export function dismissLink(id: string) {
  linkDownloads.update((l) => l.filter((x) => x.id !== id));
}
