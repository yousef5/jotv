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

export function getChannelsByType(playlistId: number, contentType: string): Promise<Channel[]> {
  return invoke<Channel[]>('get_channels_by_type', { playlistId, contentType });
}

export function getRecentlyAdded(playlistId: number, contentType: string, limit: number = 60): Promise<Channel[]> {
  return invoke<Channel[]>('get_recently_added', { playlistId, contentType, limit });
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
): Promise<void> {
  return invoke<void>('start_social_download', { url, formatId, outputDir, downloadId, title, thumbnail, platform, formatLabel });
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

// ─── Recommendation Commands ─────────────────────────────────────────────────

export function getRecommendations(limit?: number): Promise<Channel[]> {
  return invoke<Channel[]>('get_recommendations', { limit: limit ?? null });
}

// ─── Player Commands ─────────────────────────────────────────────────────────

// ─── MPV Player Commands ────────────────────────────────────────────────────

export function mpvPlay(url: string, title: string): Promise<void> {
  return invoke<void>('mpv_play', { url, title });
}

export function mpvLoad(url: string, title: string): Promise<void> {
  return invoke<void>('mpv_load', { url, title });
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
