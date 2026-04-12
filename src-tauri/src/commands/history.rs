use crate::db::models::{Channel, ViewingHistoryEntry};
use crate::db::Database;
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub async fn record_viewing(
    channel_id: i64,
    duration_seconds: i64,
    db: State<'_, Database>,
) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();

    // Look up the channel's group_name
    let group_name: String = conn
        .query_row(
            "SELECT group_name FROM channels WHERE id = ?1",
            params![channel_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO viewing_history (channel_id, duration_seconds, group_name) VALUES (?1, ?2, ?3)",
        params![channel_id, duration_seconds, group_name],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_recently_watched(
    limit: Option<i64>,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let limit = limit.unwrap_or(20);
    let conn = db.conn.lock().unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.playlist_id, c.name, c.group_name, c.stream_url, c.logo_url, c.epg_id, c.content_type, c.created_at
             FROM channels c
             INNER JOIN (
                 SELECT channel_id, MAX(started_at) as last_watched
                 FROM viewing_history
                 GROUP BY channel_id
             ) h ON c.id = h.channel_id
             ORDER BY h.last_watched DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(Channel {
                id: row.get(0)?,
                playlist_id: row.get(1)?,
                name: row.get(2)?,
                group_name: row.get(3)?,
                stream_url: row.get(4)?,
                logo_url: row.get(5)?,
                epg_id: row.get(6)?,
                content_type: row.get(7)?,
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
pub async fn get_viewing_history(
    db: State<'_, Database>,
) -> Result<Vec<ViewingHistoryEntry>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, channel_id, started_at, duration_seconds, group_name
             FROM viewing_history ORDER BY started_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ViewingHistoryEntry {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                started_at: row.get(2)?,
                duration_seconds: row.get(3)?,
                group_name: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| e.to_string())?);
    }
    Ok(entries)
}
