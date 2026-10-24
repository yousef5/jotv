// Labels for what the provider says about a title: file quality, dates, episode names.
import type { MediaQuality } from './tauri';

const DAY = 86400;

/** "4K", "1080p", "720p" or "SD". Width decides for wide films (1920×800 is 1080p). */
export function resolutionLabel(width: number | null, height: number | null): string | null {
  const w = width ?? 0;
  const h = height ?? 0;
  if (w >= 3200 || h >= 1800) return '4K';
  if (w >= 1800 || h >= 1000) return '1080p';
  if (w >= 1200 || h >= 700) return '720p';
  return w || h ? 'SD' : null;
}

function layout(channels: number): string {
  return channels === 1 ? 'Mono' : channels === 2 ? 'Stereo' : channels === 6 ? '5.1' : channels === 8 ? '7.1' : `${channels} ch`;
}

/** "Stereo", "5.1", "Dolby 5.1", "DTS 7.1" */
export function audioLabel(channels: number | null, codec: string | null): string | null {
  if (!channels) return null;
  const c = codec ?? '';
  const brand = /^(e?ac3|truehd)/.test(c) ? 'Dolby ' : c.startsWith('dts') ? 'DTS ' : '';
  return brand + layout(channels);
}

const LANGUAGES: Record<string, string> = {
  eng: 'English', ara: 'Arabic', fre: 'French', fra: 'French', ger: 'German', deu: 'German',
  spa: 'Spanish', ita: 'Italian', por: 'Portuguese', rus: 'Russian', tur: 'Turkish', hin: 'Hindi',
  jpn: 'Japanese', kor: 'Korean', chi: 'Chinese', zho: 'Chinese', per: 'Persian', fas: 'Persian',
  urd: 'Urdu', heb: 'Hebrew', dut: 'Dutch', nld: 'Dutch', gre: 'Greek', ell: 'Greek',
};

/** ISO 639-2 code ("eng") to a name; null when unknown or undetermined. */
export function languageName(code: string | null): string | null {
  const c = code?.trim().toLowerCase();
  if (!c || ['und', 'mul', 'zxx', 'mis'].includes(c)) return null;
  if (LANGUAGES[c]) return LANGUAGES[c];
  try {
    const name = new Intl.DisplayNames(['en'], { type: 'language' }).of(c);
    return name && name.toLowerCase() !== c ? name : null;
  } catch {
    return null;
  }
}

/** Short chips for the title header: ["1080p", "HDR", "Dolby 5.1", "English"] */
export function qualityTags(q: MediaQuality | null | undefined): string[] {
  if (!q) return [];
  return [
    resolutionLabel(q.width, q.height),
    q.hdr ? 'HDR' : null,
    audioLabel(q.audio_channels, q.audio_codec),
    languageName(q.audio_language),
  ].filter((t): t is string => !!t);
}

const CODECS: Record<string, string> = {
  h264: 'H.264', hevc: 'HEVC', h265: 'HEVC', av1: 'AV1', vp9: 'VP9', mpeg2video: 'MPEG-2', mpeg4: 'MPEG-4',
  aac: 'AAC', ac3: 'Dolby Digital', eac3: 'Dolby Digital Plus', truehd: 'Dolby TrueHD', dts: 'DTS',
  mp3: 'MP3', opus: 'Opus', flac: 'FLAC', vorbis: 'Vorbis',
};

function codecName(c: string | null): string | null {
  return c ? CODECS[c] ?? c.toUpperCase() : null;
}

function join(parts: (string | number | null | false | undefined)[]): string | null {
  const kept = parts.filter((p): p is string => typeof p === 'string' && !!p);
  return kept.length ? kept.join(' · ') : null;
}

/** Full description for the About section */
export function qualityDetails(q: MediaQuality | null | undefined): { video: string | null; audio: string | null } {
  if (!q) return { video: null, audio: null };
  return {
    video: join([
      q.width && q.height && `${q.width}×${q.height}`,
      codecName(q.video_codec),
      q.hdr && 'HDR',
      q.bitrate && `${(q.bitrate / 1000).toFixed(1)} Mbps`,
    ]),
    audio: join([codecName(q.audio_codec), q.audio_channels && layout(q.audio_channels), languageName(q.audio_language)]),
  };
}

/** YouTube video id from a bare id or any YouTube link. */
export function youtubeKey(v: string | null | undefined): string | null {
  const s = v?.trim();
  if (!s) return null;
  if (/^[\w-]{11}$/.test(s)) return s;
  try {
    const u = new URL(s);
    const id = u.hostname.endsWith('youtu.be')
      ? u.pathname.slice(1)
      : u.searchParams.get('v') ?? u.pathname.match(/\/(?:embed|shorts)\/([\w-]+)/)?.[1];
    return id && /^[\w-]{11}$/.test(id) ? id : null;
  } catch {
    return null;
  }
}

/** "today", "yesterday", "4 days ago", "2 weeks ago", then the date. `ts`/`now` in unix seconds. */
export function agoLabel(ts: number, now = Date.now() / 1000): string {
  const days = Math.floor((now - ts) / DAY);
  if (days < 1) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days} days ago`;
  if (days < 30) {
    const weeks = Math.floor(days / 7);
    return weeks === 1 ? 'a week ago' : `${weeks} weeks ago`;
  }
  const d = new Date(ts * 1000);
  const sameYear = d.getFullYear() === new Date(now * 1000).getFullYear();
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', ...(sameYear ? {} : { year: 'numeric' }) });
}

/** Added within the last `days` days */
export function isRecent(ts: number | null | undefined, now = Date.now() / 1000, days = 14): boolean {
  return !!ts && now - ts < days * DAY && ts <= now + DAY;
}

const GENERIC_EPISODE = /^(episode|ep\.?|الحلقة)\s*\d+$/i;

/** Episode title: TMDB's name, else the useful tail of "Show - S01E03 - Title", else "Episode 3". */
export function episodeName(raw: string, num: string, tmdbName: string | null | undefined): string {
  const t = tmdbName?.trim();
  if (t && !GENERIC_EPISODE.test(t)) return t;
  const parts = raw.split(/\s+-\s+/);
  const tail = parts[parts.length - 1]?.trim();
  return tail && parts.length > 1 && !/^S\d+E\d+$/i.test(tail) ? tail : `Episode ${num}`;
}

/** The most recently added episode across all seasons, when the server dates them. */
export function latestEpisode(
  seasons: { season_number: string; episodes: { episode_num: string; added: number | null }[] }[],
): { season: string; episode: string; added: number } | null {
  let best: { season: string; episode: string; added: number } | null = null;
  for (const s of seasons) {
    for (const e of s.episodes) {
      if (e.added && (!best || e.added > best.added)) best = { season: s.season_number, episode: e.episode_num, added: e.added };
    }
  }
  return best;
}
