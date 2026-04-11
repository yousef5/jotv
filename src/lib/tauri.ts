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
  is_vod: boolean;
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
  channel_id: number;
  url: string;
  file_path: string | null;
  status: string;
  progress: number;
  total_bytes: number | null;
  downloaded_bytes: number;
  retry_count: number;
  created_at: string;
  completed_at: string | null;
}

export interface DashboardStats {
  total_channels: number;
  total_favorites: number;
  total_playlists: number;
  total_downloads: number;
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

// ─── Recommendation Commands ─────────────────────────────────────────────────

export function getRecommendations(limit?: number): Promise<Channel[]> {
  return invoke<Channel[]>('get_recommendations', { limit: limit ?? null });
}

// ─── Player Commands ─────────────────────────────────────────────────────────

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
