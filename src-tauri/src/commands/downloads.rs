use crate::db::models::Download;
use crate::db::Database;
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub async fn queue_download(
    channel_id: i64,
    url: String,
    db: State<'_, Database>,
) -> Result<Download, String> {
    let conn = db.conn.lock().unwrap();

    conn.execute(
        "INSERT INTO downloads (channel_id, url, status) VALUES (?1, ?2, 'queued')",
        params![channel_id, url],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();

    let download = conn
        .query_row(
            "SELECT id, channel_id, url, file_path, status, progress, total_bytes,
                    downloaded_bytes, retry_count, created_at, completed_at
             FROM downloads WHERE id = ?1",
            params![id],
            |row| {
                Ok(Download {
                    id: row.get(0)?,
                    channel_id: row.get(1)?,
                    url: row.get(2)?,
                    file_path: row.get(3)?,
                    status: row.get(4)?,
                    progress: row.get(5)?,
                    total_bytes: row.get(6)?,
                    downloaded_bytes: row.get(7)?,
                    retry_count: row.get(8)?,
                    created_at: row.get(9)?,
                    completed_at: row.get(10)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(download)
}

#[tauri::command]
pub async fn get_downloads(db: State<'_, Database>) -> Result<Vec<Download>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, channel_id, url, file_path, status, progress, total_bytes,
                    downloaded_bytes, retry_count, created_at, completed_at
             FROM downloads ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Download {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                url: row.get(2)?,
                file_path: row.get(3)?,
                status: row.get(4)?,
                progress: row.get(5)?,
                total_bytes: row.get(6)?,
                downloaded_bytes: row.get(7)?,
                retry_count: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut downloads = Vec::new();
    for row in rows {
        downloads.push(row.map_err(|e| e.to_string())?);
    }
    Ok(downloads)
}

#[tauri::command]
pub async fn pause_download(id: i64, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE downloads SET status = 'paused' WHERE id = ?1 AND status IN ('queued', 'downloading')",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn resume_download(id: i64, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE downloads SET status = 'queued' WHERE id = ?1 AND status = 'paused'",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(id: i64, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();

    // Get file path before deleting
    let file_path: Option<String> = conn
        .query_row(
            "SELECT file_path FROM downloads WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;

    // Remove partial file if it exists
    if let Some(path) = file_path {
        let _ = std::fs::remove_file(path);
    }

    Ok(())
}

#[tauri::command]
pub async fn clear_completed_downloads(db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "DELETE FROM downloads WHERE status = 'completed'",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
