use crate::db::models::FavoriteChannel;
use crate::db::Database;
use rusqlite::params;
use serde::Serialize;
use tauri::State;

#[tauri::command]
pub async fn get_favorites(db: State<'_, Database>) -> Result<Vec<FavoriteChannel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.channel_id, c.name, c.group_name, c.stream_url, c.logo_url, p.name, f.category, f.added_at,
                    c.playlist_id, c.content_type, f.list_id
             FROM favorites f
             JOIN channels c ON f.channel_id = c.id
             JOIN playlists p ON c.playlist_id = p.id
             ORDER BY f.category, c.name",
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
                category: row.get(7)?,
                added_at: row.get(8)?,
                playlist_id: row.get(9)?,
                content_type: row.get(10)?,
                list_id: row.get(11)?,
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
pub async fn get_favorite_categories(db: State<'_, Database>) -> Result<Vec<String>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT DISTINCT category FROM favorites ORDER BY category")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| row.get(0)).map_err(|e| e.to_string())?;
    let mut cats = Vec::new();
    for row in rows {
        cats.push(row.map_err(|e| e.to_string())?);
    }
    Ok(cats)
}

#[tauri::command]
pub async fn add_favorite(
    channel_id: i64,
    category: String,
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
            "UPDATE favorites SET category = ?2 WHERE channel_id = ?1",
            params![channel_id, category],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "INSERT INTO favorites (channel_id, category) VALUES (?1, ?2)",
            params![channel_id, category],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[tauri::command]
pub async fn remove_favorite(
    channel_id: i64,
    db: State<'_, Database>,
) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM favorites WHERE channel_id = ?1", params![channel_id])
        .map_err(|e| e.to_string())?;
    Ok(())
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
        conn.execute("DELETE FROM favorites WHERE channel_id = ?1", params![channel_id])
            .map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        conn.execute(
            "INSERT INTO favorites (channel_id, category) VALUES (?1, 'General')",
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

// ── Your categories ("lists") for favorites ─────────────────────────────────
// Channels, movies and series each have their own set.

#[derive(Debug, Clone, Serialize)]
pub struct FavoriteList {
    pub id: i64,
    pub name: String,
    /// "live", "vod" or "series"
    pub kind: String,
    pub color: Option<String>,
    pub sort: i64,
}

fn read_list(conn: &rusqlite::Connection, id: i64) -> Result<FavoriteList, String> {
    conn.query_row("SELECT id, name, kind, color, sort FROM favorite_lists WHERE id = ?1", params![id], |r| {
        Ok(FavoriteList { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, color: r.get(3)?, sort: r.get(4)? })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn favorite_lists(db: State<'_, Database>) -> Result<Vec<FavoriteList>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, kind, color, sort FROM favorite_lists ORDER BY sort, name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok(FavoriteList { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, color: r.get(3)?, sort: r.get(4)? }))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Creates a category (no `id`) or renames / recolours one.
#[tauri::command]
pub async fn favorite_list_save(
    id: Option<i64>,
    name: String,
    kind: String,
    color: Option<String>,
    db: State<'_, Database>,
) -> Result<FavoriteList, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Category name can't be empty".into());
    }
    if !["live", "vod", "series"].contains(&kind.as_str()) {
        return Err(format!("Unknown kind: {}", kind));
    }
    let conn = db.conn.lock().unwrap();
    let id = match id {
        Some(id) => {
            conn.execute("UPDATE favorite_lists SET name = ?2, color = ?3 WHERE id = ?1", params![id, name, color])
                .map_err(|e| e.to_string())?;
            id
        }
        None => {
            let sort: i64 = conn
                .query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM favorite_lists", [], |r| r.get(0))
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO favorite_lists (name, kind, color, sort) VALUES (?1, ?2, ?3, ?4)",
                params![name, kind, color, sort],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };
    read_list(&conn, id)
}

/// Deletes a category; its favorites stay saved, just not sorted.
#[tauri::command]
pub async fn favorite_list_delete(id: i64, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("UPDATE favorites SET list_id = NULL WHERE list_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM favorite_lists WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Puts favorites into a category (None = take them out of any).
#[tauri::command]
pub async fn favorite_set_list(channel_ids: Vec<i64>, list_id: Option<i64>, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    for id in channel_ids {
        conn.execute("UPDATE favorites SET list_id = ?2 WHERE channel_id = ?1", params![id, list_id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
