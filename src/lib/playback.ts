import { detectExternalPlayers, launchExternalPlayer } from '$lib/tauri';
import type { ExternalPlayer } from '$lib/tauri';

let players: Promise<ExternalPlayer[]> | null = null;

function getPlayers(): Promise<ExternalPlayer[]> {
  players ??= detectExternalPlayers().catch(() => {
    players = null;
    return [];
  });
  return players;
}

/** Plays a VOD/episode URL in the first detected external player (MPV preferred). */
export async function playExternal(url: string, preferMpv = false): Promise<void> {
  const list = await getPlayers();
  if (list.length === 0) throw new Error('No external player found. Install MPV or VLC.');
  const player = (preferMpv && list.find((p) => /mpv/i.test(p.name))) || list[0];
  await launchExternalPlayer(player.path, url);
}

export function extFromUrl(url: string): string {
  const ext = (url.split('/').pop() ?? '').split('.').pop() ?? '';
  return ['mp4', 'mkv', 'avi', 'flv', 'ts', 'webm', 'mov'].includes(ext) ? ext : 'mp4';
}

/** "Stranglehold ( 2026 )" → { title: "Stranglehold", year: "2026" } */
export function splitTitle(name: string): { title: string; year: string | null } {
  const m = name.match(/\(\s*((?:19|20)\d{2})\s*\)/);
  const title = name.replace(/\(\s*(?:19|20)\d{2}\s*\)/, '').replace(/\s{2,}/g, ' ').trim();
  return { title: title || name, year: m?.[1] ?? null };
}

export function firstUrl(v: unknown): string | null {
  const s = Array.isArray(v) ? v.find((x) => typeof x === 'string' && x) : v;
  return typeof s === 'string' && /^https?:\/\//.test(s) ? s : null;
}

export function toRating(v: unknown): number | null {
  const n = typeof v === 'number' ? v : parseFloat(String(v ?? ''));
  return isFinite(n) && n > 0 ? Math.round(n * 10) / 10 : null;
}

export function formatRuntime(secs: number | null | undefined): string | null {
  if (!secs || secs < 60) return null;
  const h = Math.floor(secs / 3600);
  const m = Math.round((secs % 3600) / 60);
  return h ? `${h}h ${m}m` : `${m}m`;
}

/** Remote Xtream id from a VOD (…/movie/u/p/123.mkv) or series (…/series/u/p/123) URL */
export function remoteIdFromUrl(url: string): number | null {
  const n = parseInt((url.split('/').pop() ?? '').split('.')[0]);
  return isNaN(n) ? null : n;
}

/** TMDB serves any size; ask for 1280px instead of the thumbnail size the server listed. */
export function hiResImage(url: string): string {
  return url.replace(/(image\.tmdb\.org\/t\/p\/)w\d+\//, '$1w1280/');
}

/**
 * Backdrop URL worth trying full-screen. Many servers repeat the poster as the
 * backdrop, which turns into a blurry crop when stretched.
 */
export function backdropUrl(backdropPath: unknown, cover: unknown): string | null {
  const b = firstUrl(backdropPath);
  if (!b || b === firstUrl(cover)) return null;
  return hiResImage(b);
}

/** True when a loaded image is a real wide backdrop that stays sharp at full screen. */
export function isWideBackdrop(img: HTMLImageElement): boolean {
  return img.naturalWidth >= 960 && img.naturalWidth / img.naturalHeight >= 1.3;
}

/**
 * Title to search TMDB with: drops the year, quality tags and bracketed notes,
 * and keeps the Latin half of bilingual names ("Anne Yarısı - نصف الام").
 */
export function searchTitle(name: string): string {
  let t = splitTitle(name).title
    .replace(/\[[^\]]*\]|\{[^}]*\}/g, ' ')
    .replace(/\b(4K|UHD|FHD|HD|SD|HDR|Pure|Multi[- ]?Sub|Dubbed|مدبلج|مترجم)\b/gi, ' ');
  const halves = t.split(/\s+[-|–]\s+/).map((x) => x.trim()).filter(Boolean);
  const latin = halves.find((h) => /[A-Za-z]/.test(h));
  t = latin ?? halves[0] ?? t;
  return t.replace(/[()]/g, ' ').replace(/\s{2,}/g, ' ').trim();
}
