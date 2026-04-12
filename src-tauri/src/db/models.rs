use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub source_type: String,
    pub source_url: String,
    pub xtream_username: Option<String>,
    pub xtream_password: Option<String>,
    pub auto_update_interval: i64,
    pub last_updated_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: i64,
    pub playlist_id: i64,
    pub name: String,
    pub group_name: String,
    pub stream_url: String,
    pub logo_url: Option<String>,
    pub epg_id: Option<String>,
    pub content_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Favorite {
    pub id: i64,
    pub channel_id: i64,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteChannel {
    pub id: i64,
    pub channel_id: i64,
    pub channel_name: String,
    pub group_name: String,
    pub stream_url: String,
    pub logo_url: Option<String>,
    pub playlist_name: String,
    pub category: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpgEntry {
    pub id: i64,
    pub channel_epg_id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewingHistoryEntry {
    pub id: i64,
    pub channel_id: i64,
    pub started_at: String,
    pub duration_seconds: i64,
    pub group_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: i64,
    pub channel_id: Option<i64>,
    pub url: String,
    pub file_path: Option<String>,
    pub status: String,
    pub progress: f64,
    pub total_bytes: Option<i64>,
    pub downloaded_bytes: i64,
    pub retry_count: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelGroup {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentTypeCount {
    pub content_type: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_channels: i64,
    pub total_favorites: i64,
    pub total_playlists: i64,
    pub total_downloads: i64,
}
