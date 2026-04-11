use crate::db::Database;
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub async fn get_setting(
    key: String,
    db: State<'_, Database>,
) -> Result<Option<String>, String> {
    let conn = db.conn.lock().unwrap();
    let result = conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn set_setting(
    key: String,
    value: String,
    db: State<'_, Database>,
) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_all_settings(
    db: State<'_, Database>,
) -> Result<Vec<(String, String)>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings ORDER BY key")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;

    let mut settings = Vec::new();
    for row in rows {
        settings.push(row.map_err(|e| e.to_string())?);
    }
    Ok(settings)
}
