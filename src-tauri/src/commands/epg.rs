use crate::db::models::EpgEntry;
use crate::db::Database;
use crate::parsers::xmltv;
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub async fn fetch_epg(url: String, db: State<'_, Database>) -> Result<usize, String> {
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch EPG: {}", e))?;
    let xml = response
        .text()
        .await
        .map_err(|e| format!("Failed to read EPG body: {}", e))?;

    let programs = xmltv::parse_xmltv(&xml);

    let conn = db.conn.lock().unwrap();

    // Clear old EPG data
    conn.execute("DELETE FROM epg_data", [])
        .map_err(|e| e.to_string())?;

    let count = programs.len();

    for program in &programs {
        conn.execute(
            "INSERT INTO epg_data (channel_epg_id, title, description, start_time, end_time, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                program.channel_id,
                program.title,
                program.description,
                program.start,
                program.stop,
                program.category,
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(count)
}

#[tauri::command]
pub async fn get_epg_for_channel(
    epg_id: String,
    db: State<'_, Database>,
) -> Result<Vec<EpgEntry>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, channel_epg_id, title, description, start_time, end_time, category
             FROM epg_data WHERE channel_epg_id = ?1 ORDER BY start_time",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![epg_id], |row| {
            Ok(EpgEntry {
                id: row.get(0)?,
                channel_epg_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                category: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| e.to_string())?);
    }
    Ok(entries)
}

#[tauri::command]
pub async fn get_current_program(
    epg_id: String,
    db: State<'_, Database>,
) -> Result<Option<EpgEntry>, String> {
    let conn = db.conn.lock().unwrap();
    let result = conn.query_row(
        "SELECT id, channel_epg_id, title, description, start_time, end_time, category
         FROM epg_data
         WHERE channel_epg_id = ?1
           AND start_time <= datetime('now')
           AND end_time > datetime('now')
         LIMIT 1",
        params![epg_id],
        |row| {
            Ok(EpgEntry {
                id: row.get(0)?,
                channel_epg_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                category: row.get(6)?,
            })
        },
    );

    match result {
        Ok(entry) => Ok(Some(entry)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
