use crate::db::models::FavoriteChannel;
use crate::db::Database;
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub async fn get_favorites(db: State<'_, Database>) -> Result<Vec<FavoriteChannel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.channel_id, c.name, c.group_name, c.stream_url, c.logo_url, p.name, f.added_at
             FROM favorites f
             JOIN channels c ON f.channel_id = c.id
             JOIN playlists p ON c.playlist_id = p.id
             ORDER BY f.added_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(FavoriteChannel {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                channel_name: row.get(2)?,
                group_name: row.get(3)?,
                stream_url: row.get(4)?,
                logo_url: row.get(5)?,
                playlist_name: row.get(6)?,
                added_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut favorites = Vec::new();
    for row in rows {
        favorites.push(row.map_err(|e| e.to_string())?);
    }
    Ok(favorites)
}

#[tauri::command]
pub async fn toggle_favorite(
    channel_id: i64,
    db: State<'_, Database>,
) -> Result<bool, String> {
    let conn = db.conn.lock().unwrap();

    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM favorites WHERE channel_id = ?1",
            params![channel_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
        > 0;

    if exists {
        conn.execute(
            "DELETE FROM favorites WHERE channel_id = ?1",
            params![channel_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        conn.execute(
            "INSERT INTO favorites (channel_id) VALUES (?1)",
            params![channel_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
pub async fn is_favorite(
    channel_id: i64,
    db: State<'_, Database>,
) -> Result<bool, String> {
    let conn = db.conn.lock().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM favorites WHERE channel_id = ?1",
            params![channel_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}
