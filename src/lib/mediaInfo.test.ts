import { describe, it, expect } from 'vitest';
import {
  resolutionLabel, audioLabel, languageName, qualityTags, qualityDetails, youtubeKey, agoLabel, isRecent,
  episodeName, latestEpisode,
} from './mediaInfo';
import type { MediaQuality } from './tauri';

const q = (over: Partial<MediaQuality>): MediaQuality => ({
  width: null, height: null, video_codec: null, hdr: false, audio_codec: null,
  audio_channels: null, audio_language: null, bitrate: null, ...over,
});

const DAY = 86400;
const NOW = Date.UTC(2026, 9, 19, 12) / 1000;

describe('resolutionLabel', () => {
  it('reads cinema-scope widths as their real resolution', () => {
    expect(resolutionLabel(1920, 800)).toBe('1080p');
    expect(resolutionLabel(3840, 1600)).toBe('4K');
    expect(resolutionLabel(1280, 534)).toBe('720p');
    expect(resolutionLabel(720, 480)).toBe('SD');
    expect(resolutionLabel(null, null)).toBeNull();
  });
});

describe('audioLabel', () => {
  it('names channel layouts and Dolby codecs', () => {
    expect(audioLabel(2, 'aac')).toBe('Stereo');
    expect(audioLabel(6, 'eac3')).toBe('Dolby 5.1');
    expect(audioLabel(8, 'truehd')).toBe('Dolby 7.1');
    expect(audioLabel(6, 'dts')).toBe('DTS 5.1');
    expect(audioLabel(1, 'aac')).toBe('Mono');
    expect(audioLabel(null, 'aac')).toBeNull();
  });
});

describe('languageName', () => {
  it('turns ISO 639-2 codes into names', () => {
    expect(languageName('eng')).toBe('English');
    expect(languageName('ara')).toBe('Arabic');
    expect(languageName('fre')).toBe('French');
    expect(languageName('und')).toBeNull();
    expect(languageName(null)).toBeNull();
  });
});

describe('qualityTags', () => {
  it('lists resolution, HDR, sound and language', () => {
    expect(qualityTags(q({ width: 1920, height: 800, audio_channels: 2, audio_codec: 'aac', audio_language: 'eng' })))
      .toEqual(['1080p', 'Stereo', 'English']);
    expect(qualityTags(q({ width: 3840, height: 2160, hdr: true, audio_channels: 6, audio_codec: 'eac3' })))
      .toEqual(['4K', 'HDR', 'Dolby 5.1']);
    expect(qualityTags(null)).toEqual([]);
  });

  it('describes the file for the details list', () => {
    expect(qualityDetails(q({ width: 1920, height: 800, video_codec: 'h264', bitrate: 4965, audio_codec: 'aac', audio_channels: 2, audio_language: 'eng' })))
      .toEqual({ video: '1920×800 · H.264 · 5.0 Mbps', audio: 'AAC · Stereo · English' });
    expect(qualityDetails(q({ video_codec: 'hevc', hdr: true }))).toEqual({ video: 'HEVC · HDR', audio: null });
  });
});

describe('youtubeKey', () => {
  it('accepts bare keys and YouTube links', () => {
    expect(youtubeKey('7a3mjy9BXec')).toBe('7a3mjy9BXec');
    expect(youtubeKey('https://www.youtube.com/watch?v=7a3mjy9BXec&t=3')).toBe('7a3mjy9BXec');
    expect(youtubeKey('https://youtu.be/7a3mjy9BXec')).toBe('7a3mjy9BXec');
    expect(youtubeKey('  ')).toBeNull();
    expect(youtubeKey('not a key!')).toBeNull();
  });
});

describe('dates', () => {
  it('says how long ago something happened', () => {
    expect(agoLabel(NOW - 3600, NOW)).toBe('today');
    expect(agoLabel(NOW - DAY - 60, NOW)).toBe('yesterday');
    expect(agoLabel(NOW - 4 * DAY, NOW)).toBe('4 days ago');
    expect(agoLabel(NOW - 15 * DAY, NOW)).toBe('2 weeks ago');
    expect(agoLabel(NOW - 90 * DAY, NOW)).toBe('Jul 21');
  });

  it('treats the last two weeks as new', () => {
    expect(isRecent(NOW - 13 * DAY, NOW)).toBe(true);
    expect(isRecent(NOW - 15 * DAY, NOW)).toBe(false);
    expect(isRecent(null, NOW)).toBe(false);
  });
});

describe('episodeName', () => {
  it('prefers a real TMDB name', () => {
    expect(episodeName('Tracker.2024.S01E01', '1', 'Klamath Falls')).toBe('Klamath Falls');
  });
  it('ignores generic names and release-style titles', () => {
    expect(episodeName('Tracker.2024.S01E01', '1', 'Episode 1')).toBe('Episode 1');
    expect(episodeName('Tracker.2024.S01E01', '1', null)).toBe('Episode 1');
  });
  it('keeps the useful tail of "Show - S01E03 - Title"', () => {
    expect(episodeName('Show - S01E03 - The Return', '3', null)).toBe('The Return');
    expect(episodeName('Show - S01E03', '3', null)).toBe('Episode 3');
  });
});

describe('latestEpisode', () => {
  it('finds the newest episode across seasons', () => {
    const ep = (id: number, added: number | null) => ({ id, episode_num: String(id), added });
    const seasons = [
      { season_number: '1', episodes: [ep(1, NOW - 100 * DAY), ep(2, NOW - 90 * DAY)] },
      { season_number: '2', episodes: [ep(3, NOW - 2 * DAY), ep(4, null)] },
    ];
    expect(latestEpisode(seasons)).toEqual({ season: '2', episode: '3', added: NOW - 2 * DAY });
    expect(latestEpisode([{ season_number: '1', episodes: [ep(1, null)] }])).toBeNull();
  });
});
