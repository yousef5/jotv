use crate::db::models::{Channel, DashboardStats};
use crate::db::Database;
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub async fn get_channels(
    playlist_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, is_vod, created_at
             FROM channels WHERE playlist_id = ?1 ORDER BY group_name, name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id], |row| {
            Ok(Channel {
                id: row.get(0)?,
                playlist_id: row.get(1)?,
                name: row.get(2)?,
                group_name: row.get(3)?,
                stream_url: row.get(4)?,
                logo_url: row.get(5)?,
                epg_id: row.get(6)?,
                is_vod: row.get::<_, i32>(7)? != 0,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut channels = Vec::new();
    for row in rows {
        channels.push(row.map_err(|e| e.to_string())?);
    }
    Ok(channels)
}

#[tauri::command]
pub async fn search_channels(
    query: String,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();
    let pattern = format!("%{}%", query);
    let mut stmt = conn
        .prepare(
            "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, is_vod, created_at
             FROM channels WHERE name LIKE ?1 OR group_name LIKE ?1 LIMIT 100",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![pattern], |row| {
            Ok(Channel {
                id: row.get(0)?,
                playlist_id: row.get(1)?,
                name: row.get(2)?,
                group_name: row.get(3)?,
                stream_url: row.get(4)?,
                logo_url: row.get(5)?,
                epg_id: row.get(6)?,
                is_vod: row.get::<_, i32>(7)? != 0,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut channels = Vec::new();
    for row in rows {
        channels.push(row.map_err(|e| e.to_string())?);
    }
    Ok(channels)
}

#[tauri::command]
pub async fn get_dashboard_stats(db: State<'_, Database>) -> Result<DashboardStats, String> {
    let conn = db.conn.lock().unwrap();

    let total_channels: i64 = conn
        .query_row("SELECT COUNT(*) FROM channels", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_favorites: i64 = conn
        .query_row("SELECT COUNT(*) FROM favorites", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_playlists: i64 = conn
        .query_row("SELECT COUNT(*) FROM playlists", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_downloads: i64 = conn
        .query_row("SELECT COUNT(*) FROM downloads", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    Ok(DashboardStats {
        total_channels,
        total_favorites,
        total_playlists,
        total_downloads,
    })
}
