import { invoke } from '@tauri-apps/api/core';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface Playlist {
  id: number;
  name: string;
  source_type: string;
  source_url: string;
  xtream_username: string | null;
  xtream_password: string | null;
  auto_update_interval: number;
  last_updated_at: string | null;
  created_at: string;
}

export interface Channel {
  id: number;
  playlist_id: number;
  name: string;
  group_name: string;
  stream_url: string;
  logo_url: string | null;
  epg_id: string | null;
  content_type: 'live' | 'vod' | 'series';
  created_at: string;
}

export interface ChannelGroup {
  name: string;
  count: number;
}

export interface FavoriteChannel {
  id: number;
  channel_id: number;
  channel_name: string;
  group_name: string;
  stream_url: string;
  logo_url: string | null;
  playlist_name: string;
  category: string;
  added_at: string;
  playlist_id: number;
  content_type: 'live' | 'vod' | 'series';
  /** Your category (FavoriteList id); null = not sorted yet */
  list_id: number | null;
}

export interface EpgEntry {
  id: number;
  channel_epg_id: string;
  title: string;
  description: string | null;
  start_time: string;
  end_time: string;
  category: string | null;
}

export interface ViewingHistoryEntry {
  id: number;
  channel_id: number;
  started_at: string;
  duration_seconds: number;
  group_name: string;
}

export interface Download {
  id: number;
  channel_id: number | null;
  url: string;
  file_path: string | null;
  status: string;
  progress: number;
  total_bytes: number | null;
  downloaded_bytes: number;
  retry_count: number;
  created_at: string;
  completed_at: string | null;
  channel_name: string | null;
  channel_logo: string | null;
}

export interface DashboardStats {
  total_channels: number;
  total_favorites: number;
  total_playlists: number;
  total_downloads: number;
}

export interface ContentTypeCount {
  content_type: string;
  count: number;
}

export interface ExternalPlayer {
  name: string;
  path: string;
}

// ─── Playlist Commands ───────────────────────────────────────────────────────

export function getPlaylists(): Promise<Playlist[]> {
  return invoke<Playlist[]>('get_playlists');
}

export function addPlaylistFromUrl(name: string, url: string): Promise<Playlist> {
  return invoke<Playlist>('add_playlist_from_url', { name, url });
}

export function addPlaylistFromFile(name: string, filePath: string): Promise<Playlist> {
  return invoke<Playlist>('add_playlist_from_file', { name, filePath });
}

export function addPlaylistFromXtream(
  name: string,
  server: string,
  username: string,
  password: string
): Promise<Playlist> {
  return invoke<Playlist>('add_playlist_from_xtream', { name, server, username, password });
}

export interface XtreamAccountInfo {
  username: string;
  status: string;
  exp_date: string | null;
  is_trial: string | null;
  active_cons: string | null;
  max_connections: string | null;
  created_at: string | null;
}

export function getXtreamAccountInfo(
  server: string,
  username: string,
  password: string
): Promise<XtreamAccountInfo> {
  return invoke<XtreamAccountInfo>('get_xtream_account_info', { server, username, password });
}

export interface SeriesInfo {
  name: string | null;
  cover: string | null;
  plot: string | null;
  cast: string | null;
  director: string | null;
  genre: string | null;
  release_date: string | null;
  rating: unknown;
  backdrop_path: unknown;
}

export interface EpisodeDetail {
  id: number;
  episode_num: string;
  title: string;
  stream_url: string;
  container_extension: string;
  image: string | null;
  plot: string | null;
  duration_secs: number | null;
}

export interface SeasonDetail {
  season_number: string;
  episodes: EpisodeDetail[];
}

export interface SeriesDetail {
  info: SeriesInfo;
  seasons: SeasonDetail[];
}

export interface VodInfo {
  name: string | null;
  cover: string | null;
  plot: string | null;
  cast: string | null;
  director: string | null;
  genre: string | null;
  release_date: string | null;
  rating: unknown;
  backdrop_path: unknown;
  duration: string | null;
  duration_secs: number | null;
  youtube_trailer: string | null;
}

export interface VodDetail {
  info: VodInfo;
}

export function getVodInfo(
  server: string,
  username: string,
  password: string,
  vodId: number
): Promise<VodDetail> {
  return invoke<VodDetail>('get_vod_info', { server, username, password, vodId });
}

export function getSeriesInfo(
  server: string,
  username: string,
  password: string,
  seriesId: number
): Promise<SeriesDetail> {
  return invoke<SeriesDetail>('get_series_info', { server, username, password, seriesId });
}

export function refreshPlaylist(id: number): Promise<Playlist> {
  return invoke<Playlist>('refresh_playlist', { id });
}

export function deletePlaylist(id: number): Promise<void> {
  return invoke<void>('delete_playlist', { id });
}

export function getPlaylistGroups(playlistId: number): Promise<ChannelGroup[]> {
  return invoke<ChannelGroup[]>('get_playlist_groups', { playlistId });
}

export function mergePlaylists(sourceIds: number[], targetName: string): Promise<Playlist> {
  return invoke<Playlist>('merge_playlists', { sourceIds, targetName });
}

export function splitPlaylist(playlistId: number): Promise<Playlist[]> {
  return invoke<Playlist[]>('split_playlist', { playlistId });
}

export function exportPlaylist(playlistId: number, outputPath: string): Promise<void> {
  return invoke<void>('export_playlist', { playlistId, outputPath });
}

// ─── Channel Commands ────────────────────────────────────────────────────────

export function getChannels(playlistId: number): Promise<Channel[]> {
  return invoke<Channel[]>('get_channels', { playlistId });
}

/** A movie/series with list metadata (rating, year, genre) */
export interface MediaChannel extends Channel {
  rating: number | null;
  year: number | null;
  genre: string | null;
}

export type MediaSort = 'added' | 'rating' | 'year' | 'name';

export interface MediaQuery {
  group?: string | null;
  sort?: MediaSort;
  year?: number | null;
  genre?: string | null;
  minRating?: number | null;
}

export function browseMedia(
  playlistId: number,
  contentType: string,
  q: MediaQuery,
  limit: number,
  offset = 0,
): Promise<MediaChannel[]> {
  return invoke<MediaChannel[]>('browse_media', {
    playlistId,
    contentType,
    group: q.group ?? null,
    sort: q.sort ?? 'added',
    year: q.year ?? null,
    genre: q.genre ?? null,
    minRating: q.minRating ?? null,
    limit,
    offset,
  });
}

export interface MediaFacets {
  years: { value: number; count: number }[];
  genres: { value: string; count: number }[];
  /** How many titles have a rating at all */
  rated: number;
}

export function getMediaFacets(playlistId: number, contentType: string, group: string | null): Promise<MediaFacets> {
  return invoke<MediaFacets>('get_media_facets', { playlistId, contentType, group });
}

export function getChannel(id: number): Promise<Channel> {
  return invoke<Channel>('get_channel', { id });
}

export function getChannelsByType(playlistId: number, contentType: string): Promise<Channel[]> {
  return invoke<Channel[]>('get_channels_by_type', { playlistId, contentType });
}

export function getRecentlyAdded(playlistId: number, contentType: string, limit: number = 60, offset: number = 0): Promise<Channel[]> {
  return invoke<Channel[]>('get_recently_added', { playlistId, contentType, limit, offset });
}

export function getGroupsByType(playlistId: number, contentType: string): Promise<ChannelGroup[]> {
  return invoke<ChannelGroup[]>('get_groups_by_type', { playlistId, contentType });
}

export function getChannelsByGroup(
  playlistId: number,
  contentType: string,
  groupName: string,
  limit: number,
  offset: number
): Promise<Channel[]> {
  return invoke<Channel[]>('get_channels_by_group', { playlistId, contentType, groupName, limit, offset });
}

export function getContentTypeCounts(playlistId: number): Promise<ContentTypeCount[]> {
  return invoke<ContentTypeCount[]>('get_content_type_counts', { playlistId });
}

export function searchChannelsInPlaylist(
  playlistId: number,
  contentType: string,
  query: string,
  limit: number = 100
): Promise<Channel[]> {
  return invoke<Channel[]>('search_channels_in_playlist', { playlistId, contentType, query, limit });
}

export function searchChannels(query: string): Promise<Channel[]> {
  return invoke<Channel[]>('search_channels', { query });
}

export function getDashboardStats(): Promise<DashboardStats> {
  return invoke<DashboardStats>('get_dashboard_stats');
}

// ─── Favorite Commands ───────────────────────────────────────────────────────

export function getFavorites(): Promise<FavoriteChannel[]> {
  return invoke<FavoriteChannel[]>('get_favorites');
}

export function getFavoriteCategories(): Promise<string[]> {
  return invoke<string[]>('get_favorite_categories');
}

export function addFavorite(channelId: number, category: string): Promise<boolean> {
  return invoke<boolean>('add_favorite', { channelId, category });
}

export function removeFavorite(channelId: number): Promise<void> {
  return invoke<void>('remove_favorite', { channelId });
}

export function toggleFavorite(channelId: number): Promise<boolean> {
  return invoke<boolean>('toggle_favorite', { channelId });
}

export interface FavoriteList {
  id: number;
  name: string;
  kind: 'live' | 'vod' | 'series';
  color: string | null;
  sort: number;
}

export function favoriteLists(): Promise<FavoriteList[]> {
  return invoke<FavoriteList[]>('favorite_lists');
}

export function favoriteListSave(id: number | null, name: string, kind: FavoriteList['kind'], color: string | null = null): Promise<FavoriteList> {
  return invoke<FavoriteList>('favorite_list_save', { id, name, kind, color });
}

export function favoriteListDelete(id: number): Promise<void> {
  return invoke<void>('favorite_list_delete', { id });
}

export function favoriteSetList(channelIds: number[], listId: number | null): Promise<void> {
  return invoke<void>('favorite_set_list', { channelIds, listId });
}

export function isFavorite(channelId: number): Promise<boolean> {
  return invoke<boolean>('is_favorite', { channelId });
}

// ─── History Commands ────────────────────────────────────────────────────────

export function recordViewing(channelId: number, durationSeconds: number): Promise<void> {
  return invoke<void>('record_viewing', { channelId, durationSeconds });
}

export function getRecentlyWatched(limit?: number): Promise<Channel[]> {
  return invoke<Channel[]>('get_recently_watched', { limit: limit ?? null });
}

export function getViewingHistory(): Promise<ViewingHistoryEntry[]> {
  return invoke<ViewingHistoryEntry[]>('get_viewing_history');
}

// ─── EPG Commands ────────────────────────────────────────────────────────────

export function fetchEpg(url: string): Promise<number> {
  return invoke<number>('fetch_epg', { url });
}

export function getEpgForChannel(epgId: string): Promise<EpgEntry[]> {
  return invoke<EpgEntry[]>('get_epg_for_channel', { epgId });
}

export function getCurrentProgram(epgId: string): Promise<EpgEntry | null> {
  return invoke<EpgEntry | null>('get_current_program', { epgId });
}

// ─── Download Commands ───────────────────────────────────────────────────────

export function startDownload(url: string, filename: string, channelId: number): Promise<Download> {
  return invoke<Download>('start_download', { url, filename, channelId });
}

export function queueDownload(channelId: number, url: string): Promise<Download> {
  return invoke<Download>('queue_download', { channelId, url });
}

export function getDownloads(): Promise<Download[]> {
  return invoke<Download[]>('get_downloads');
}

export function pauseDownload(id: number): Promise<void> {
  return invoke<void>('pause_download', { id });
}

export function resumeDownload(id: number): Promise<void> {
  return invoke<void>('resume_download', { id });
}

export function cancelDownload(id: number): Promise<void> {
  return invoke<void>('cancel_download', { id });
}

export function clearCompletedDownloads(): Promise<void> {
  return invoke<void>('clear_completed_downloads');
}

export function openDownloadFile(filePath: string): Promise<void> {
  return invoke<void>('open_download_file', { filePath });
}

export function showInFolder(filePath: string): Promise<void> {
  return invoke<void>('show_in_folder', { filePath });
}

export function getDefaultDownloadDir(): Promise<string> {
  return invoke<string>('get_default_download_dir');
}

// ─── Social Download Commands ───────────────────────────────────────────────

export interface SocialFormat {
  format_id: string;
  ext: string;
  resolution: string | null;
  filesize: number | null;
  vcodec: string | null;
  acodec: string | null;
  label: string;
}

export interface SocialVideoInfo {
  id: string;
  title: string;
  thumbnail: string | null;
  duration: number | null;
  uploader: string | null;
  view_count: number | null;
  formats: SocialFormat[];
  url: string;
  platform: string;
}

export interface SocialDownloadProgress {
  download_id: string;
  progress: number;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed: string | null;
  eta: string | null;
  status: string;
  filename: string | null;
}

export function checkYtdlp(): Promise<string> {
  return invoke<string>('check_ytdlp');
}

export function getSocialVideoInfo(url: string): Promise<SocialVideoInfo> {
  return invoke<SocialVideoInfo>('get_social_video_info', { url });
}

export function startSocialDownload(
  url: string,
  formatId: string,
  outputDir: string,
  downloadId: string,
  title: string,
  thumbnail: string | null,
  platform: string,
  formatLabel: string | null,
  categoryId: number | null = null,
  /** Exact tags; `null` = use the title's #hashtags */
  tags: string[] | null = null,
): Promise<void> {
  return invoke<void>('start_social_download', { url, formatId, outputDir, downloadId, title, thumbnail, platform, formatLabel, categoryId, tags });
}

export function cancelSocialDownload(downloadId: string): Promise<void> {
  return invoke<void>('cancel_social_download', { downloadId });
}

export interface SocialDownloadRecord {
  id: string;
  url: string;
  title: string;
  thumbnail: string | null;
  platform: string;
  format_label: string | null;
  output_dir: string | null;
  file_path: string | null;
  status: string;
  progress: number;
  downloaded_bytes: number;
  total_bytes: number | null;
  created_at: string;
  completed_at: string | null;
}

export function getSocialDownloads(): Promise<SocialDownloadRecord[]> {
  return invoke<SocialDownloadRecord[]>('get_social_downloads');
}

export function clearSocialDownloads(): Promise<void> {
  return invoke<void>('clear_social_downloads');
}

// ─── Video Library (reels) ───────────────────────────────────────────────────

export interface Reel {
  id: string;
  /** Display name (the user's name, else the original title) */
  title: string;
  original_title: string;
  url: string;
  platform: string;
  file_path: string;
  thumbnail: string | null;
  thumb_path: string | null;
  category_id: number | null;
  tags: string[];
  favorite: boolean;
  duration: number | null;
  width: number | null;
  height: number | null;
  file_size: number | null;
  plays: number;
  last_played_at: string | null;
  created_at: string;
  kind: 'video' | 'image';
}

export interface ReelCategory {
  id: number;
  name: string;
  parent_id: number | null;
  color: string | null;
  sort: number;
}

export function reelsList(): Promise<Reel[]> {
  return invoke<Reel[]>('reels_list');
}

export function reelUpdate(id: string, name: string | null, categoryId: number | null, tags: string[], favorite: boolean): Promise<Reel> {
  return invoke<Reel>('reel_update', { id, name, categoryId, tags, favorite });
}

export function reelsMove(ids: string[], categoryId: number | null): Promise<void> {
  return invoke<void>('reels_move', { ids, categoryId });
}

export function reelsTag(ids: string[], tags: string[]): Promise<void> {
  return invoke<void>('reels_tag', { ids, tags });
}

export function reelsDelete(ids: string[], deleteFiles: boolean): Promise<void> {
  return invoke<void>('reels_delete', { ids, deleteFiles });
}

export function reelsImport(paths: string[], categoryId: number | null): Promise<number> {
  return invoke<number>('reels_import', { paths, categoryId });
}

export function reelProbe(id: string): Promise<Reel> {
  return invoke<Reel>('reel_probe', { id });
}

/** Use an image as a video's cover; `null` goes back to a frame from the video. */
export function reelSetCover(id: string, image: string | null): Promise<Reel> {
  return invoke<Reel>('reel_set_cover', { id, image });
}

export function reelPlayed(id: string): Promise<void> {
  return invoke<void>('reel_played', { id });
}

export function reelCategories(): Promise<ReelCategory[]> {
  return invoke<ReelCategory[]>('reel_categories');
}

export function reelCategorySave(id: number | null, name: string, parentId: number | null, color: string | null): Promise<ReelCategory> {
  return invoke<ReelCategory>('reel_category_save', { id, name, parentId, color });
}

export function reelCategoryDelete(id: number): Promise<void> {
  return invoke<void>('reel_category_delete', { id });
}

// ─── Recommendation Commands ─────────────────────────────────────────────────

export function getRecommendations(limit?: number): Promise<Channel[]> {
  return invoke<Channel[]>('get_recommendations', { limit: limit ?? null });
}

// ─── Player Commands ─────────────────────────────────────────────────────────

// ─── MPV Player Commands ────────────────────────────────────────────────────

export interface MpvOptions {
  /** Native window to render into (in-app playback) */
  wid?: number;
  volume?: number;
  muted?: boolean;
  /** false for movies/episodes: no LIVE badge, seekable, MPV seek bar in fullscreen */
  live?: boolean;
  /** Start position in seconds (resume) */
  start?: number;
  /** Preferred audio language(s), e.g. "ara" */
  alang?: string;
  /** Preferred subtitle language(s), or "no" for subtitles off */
  slang?: string;
}

function mpvArgs(url: string, title: string, o: MpvOptions) {
  return {
    url,
    title,
    wid: o.wid ?? null,
    volume: o.volume ?? null,
    muted: o.muted ?? null,
    live: o.live ?? null,
    start: o.start ?? null,
    alang: o.alang ?? null,
    slang: o.slang ?? null,
  };
}

export function mpvPlay(url: string, title: string, opts: MpvOptions = {}): Promise<void> {
  return invoke<void>('mpv_play', mpvArgs(url, title, opts));
}

export function mpvLoad(url: string, title: string, opts: MpvOptions = {}): Promise<void> {
  return invoke<void>('mpv_load', mpvArgs(url, title, opts));
}

/** Seek by `seconds`, or to `seconds` when `absolute`. */
export function mpvSeek(seconds: number, absolute = false): Promise<void> {
  return invoke<void>('mpv_seek', { seconds, absolute });
}

export interface MpvTrack {
  id: number;
  kind: 'audio' | 'sub';
  lang: string | null;
  title: string | null;
  codec: string | null;
  channels: number | null;
  selected: boolean;
  external: boolean;
}

export function mpvTracks(): Promise<MpvTrack[]> {
  return invoke<MpvTrack[]>('mpv_tracks');
}

/** `id` null = subtitles off. `lang` becomes the preference for the next file. */
export function mpvSetTrack(kind: 'audio' | 'sub', id: number | null, lang: string | null): Promise<void> {
  return invoke<void>('mpv_set_track', { kind, id, lang });
}

/** Shows or hides MPV's own seek bar (in-app fullscreen). */
export function mpvOsc(visible: boolean): Promise<void> {
  return invoke<void>('mpv_osc', { visible });
}

export function mpvToggleMute(): Promise<void> {
  return invoke<void>('mpv_toggle_mute');
}

export function mpvPause(): Promise<void> {
  return invoke<void>('mpv_pause');
}

export function mpvStop(): Promise<void> {
  return invoke<void>('mpv_stop');
}

export function mpvFullscreen(): Promise<void> {
  return invoke<void>('mpv_fullscreen');
}

export function mpvVolume(volume: number): Promise<void> {
  return invoke<void>('mpv_volume', { volume });
}

export function mpvIsRunning(): Promise<boolean> {
  return invoke<boolean>('mpv_is_running');
}

export type MpvPlaybackState = 'live' | 'buffering' | 'nosignal' | 'paused' | 'offline';

export interface MpvStatus {
  state: MpvPlaybackState;
  cache_seconds: number;
  /** Seconds downloaded but not yet shown: how far playback trails the live edge */
  behind_seconds: number;
  volume: number;
  muted: boolean;
  time_pos: number;
  /** 0 for live streams */
  duration: number;
  eof: boolean;
}

export interface ProbeResult {
  ok: boolean;
  status: number | null;
  message: string;
}

// ─── In-app video surface (MPV --wid) ──────────────────────────────────────

/** Native window id MPV can render into; rejects where embedding isn't supported. */
export function embedAttach(): Promise<number> {
  return invoke<number>('embed_attach');
}

/** Positions the video surface, in CSS pixels relative to the webview. */
export function embedPlace(x: number, y: number, width: number, height: number, visible: boolean): Promise<void> {
  return invoke<void>('embed_place', { x, y, width, height, visible });
}

export function embedDetach(): Promise<void> {
  return invoke<void>('embed_detach');
}

// Live connection status of the channel currently playing in MPV.
export function mpvStatus(): Promise<MpvStatus> {
  return invoke<MpvStatus>('mpv_status');
}

/** Fresh connection to `url` exactly as given (stream recovery). */
export function mpvReconnect(url: string): Promise<void> {
  return invoke<void>('mpv_reconnect', { url });
}

/** "stable" = bigger buffer before resuming after a stall; "fast" = resume quickly. */
export function mpvSetBuffer(mode: 'stable' | 'fast'): Promise<void> {
  return invoke<void>('mpv_set_buffer', { mode });
}

export function mpvBadgeReconnecting(on: boolean): Promise<void> {
  return invoke<void>('mpv_badge_reconnecting', { on });
}

/** Jumps to the live edge of what MPV has already downloaded. */
export function mpvGoLive(): Promise<void> {
  return invoke<void>('mpv_go_live');
}

// Test whether a stream URL is reachable and serving data (does not touch MPV).
export function probeStream(url: string): Promise<ProbeResult> {
  return invoke<ProbeResult>('probe_stream', { url });
}

// ─── External Player Commands ───────────────────────────────────────────────

export function launchExternalPlayer(playerPath: string, streamUrl: string): Promise<void> {
  return invoke<void>('launch_external_player', { playerPath, streamUrl });
}

export function detectExternalPlayers(): Promise<ExternalPlayer[]> {
  return invoke<ExternalPlayer[]>('detect_external_players');
}

// ─── Settings Commands ───────────────────────────────────────────────────────

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>('get_setting', { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>('set_setting', { key, value });
}

export function getAllSettings(): Promise<[string, string][]> {
  return invoke<[string, string][]>('get_all_settings');
}

// ─── TMDB artwork (optional, needs the user's key) ──────────────────────────

export interface TmdbPerson {
  name: string;
  character: string | null;
  photo: string | null;
}

export interface TmdbDetails {
  tmdb_id: number;
  backdrop: string | null;
  /** Transparent title treatment */
  logo: string | null;
  poster: string | null;
  overview: string | null;
  rating: number | null;
  /** YouTube key */
  trailer: string | null;
  cast: TmdbPerson[];
}

/** Null when no TMDB key is set or nothing matched. */
export function tmdbDetails(kind: 'vod' | 'series', title: string, year: number | null): Promise<TmdbDetails | null> {
  return invoke<TmdbDetails | null>('tmdb_details', { kind, title, year });
}

export function tmdbCheckKey(key: string): Promise<boolean> {
  return invoke<boolean>('tmdb_check_key', { key });
}

// ─── Global search (all playlists, in-memory index) ─────────────────────────

export interface GlobalSearchResult {
  live: MediaChannel[];
  vod: MediaChannel[];
  series: MediaChannel[];
  /** All matches per type; the lists hold the best few */
  counts: { live: number; vod: number; series: number };
  elapsed_ms: number;
}

export function globalSearch(query: string, perType = 12): Promise<GlobalSearchResult> {
  return invoke<GlobalSearchResult>('global_search', { query, perType });
}

/** Builds the search index ahead of the first query. */
export function searchWarmup(): Promise<void> {
  return invoke<void>('search_warmup');
}
