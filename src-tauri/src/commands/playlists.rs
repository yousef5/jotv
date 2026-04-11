use crate::db::models::{ChannelGroup, Playlist};
use crate::db::Database;
use crate::parsers::m3u;
use crate::parsers::xtream::{self, XtreamCredentials};
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub async fn get_playlists(db: State<'_, Database>) -> Result<Vec<Playlist>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, source_type, source_url, xtream_username, xtream_password,
                    auto_update_interval, last_updated_at, created_at
             FROM playlists ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                source_type: row.get(2)?,
                source_url: row.get(3)?,
                xtream_username: row.get(4)?,
                xtream_password: row.get(5)?,
                auto_update_interval: row.get(6)?,
                last_updated_at: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut playlists = Vec::new();
    for row in rows {
        playlists.push(row.map_err(|e| e.to_string())?);
    }
    Ok(playlists)
}

#[tauri::command]
pub async fn add_playlist_from_url(
    name: String,
    url: String,
    db: State<'_, Database>,
) -> Result<Playlist, String> {
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch M3U: {}", e))?;
    let content = response
        .text()
        .await
        .map_err(|e| format!("Failed to read M3U body: {}", e))?;

    let entries = m3u::parse_m3u(&content);

    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO playlists (name, source_type, source_url) VALUES (?1, 'm3u_url', ?2)",
        params![name, url],
    )
    .map_err(|e| e.to_string())?;

    let playlist_id = conn.last_insert_rowid();

    for entry in &entries {
        conn.execute(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, is_vod)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                playlist_id,
                entry.name,
                entry.group,
                entry.stream_url,
                entry.logo_url,
                entry.epg_id,
                entry.is_vod as i32,
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    let playlist = conn
        .query_row(
            "SELECT id, name, source_type, source_url, xtream_username, xtream_password,
                    auto_update_interval, last_updated_at, created_at
             FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source_type: row.get(2)?,
                    source_url: row.get(3)?,
                    xtream_username: row.get(4)?,
                    xtream_password: row.get(5)?,
                    auto_update_interval: row.get(6)?,
                    last_updated_at: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(playlist)
}

#[tauri::command]
pub async fn add_playlist_from_file(
    name: String,
    file_path: String,
    db: State<'_, Database>,
) -> Result<Playlist, String> {
    let content =
        std::fs::read_to_string(&file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let entries = m3u::parse_m3u(&content);

    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO playlists (name, source_type, source_url) VALUES (?1, 'm3u_file', ?2)",
        params![name, file_path],
    )
    .map_err(|e| e.to_string())?;

    let playlist_id = conn.last_insert_rowid();

    for entry in &entries {
        conn.execute(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, is_vod)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                playlist_id,
                entry.name,
                entry.group,
                entry.stream_url,
                entry.logo_url,
                entry.epg_id,
                entry.is_vod as i32,
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    let playlist = conn
        .query_row(
            "SELECT id, name, source_type, source_url, xtream_username, xtream_password,
                    auto_update_interval, last_updated_at, created_at
             FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source_type: row.get(2)?,
                    source_url: row.get(3)?,
                    xtream_username: row.get(4)?,
                    xtream_password: row.get(5)?,
                    auto_update_interval: row.get(6)?,
                    last_updated_at: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(playlist)
}

#[tauri::command]
pub async fn add_playlist_from_xtream(
    name: String,
    server: String,
    username: String,
    password: String,
    db: State<'_, Database>,
) -> Result<Playlist, String> {
    let creds = XtreamCredentials {
        server: server.clone(),
        username: username.clone(),
        password: password.clone(),
    };

    let channels = xtream::fetch_xtream_channels(&creds).await?;

    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO playlists (name, source_type, source_url, xtream_username, xtream_password)
         VALUES (?1, 'xtream', ?2, ?3, ?4)",
        params![name, server, username, password],
    )
    .map_err(|e| e.to_string())?;

    let playlist_id = conn.last_insert_rowid();

    for ch in &channels {
        conn.execute(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, is_vod)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                playlist_id,
                ch.name,
                ch.group,
                ch.stream_url,
                ch.logo_url,
                ch.epg_id,
                ch.is_vod as i32,
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    let playlist = conn
        .query_row(
            "SELECT id, name, source_type, source_url, xtream_username, xtream_password,
                    auto_update_interval, last_updated_at, created_at
             FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source_type: row.get(2)?,
                    source_url: row.get(3)?,
                    xtream_username: row.get(4)?,
                    xtream_password: row.get(5)?,
                    auto_update_interval: row.get(6)?,
                    last_updated_at: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(playlist)
}

#[tauri::command]
pub async fn delete_playlist(id: i64, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_playlist_groups(
    playlist_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<ChannelGroup>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT group_name, COUNT(*) as count FROM channels
             WHERE playlist_id = ?1 GROUP BY group_name ORDER BY group_name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id], |row| {
            Ok(ChannelGroup {
                name: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut groups = Vec::new();
    for row in rows {
        groups.push(row.map_err(|e| e.to_string())?);
    }
    Ok(groups)
}

#[tauri::command]
pub async fn merge_playlists(
    source_ids: Vec<i64>,
    target_name: String,
    db: State<'_, Database>,
) -> Result<Playlist, String> {
    let conn = db.conn.lock().unwrap();

    conn.execute(
        "INSERT INTO playlists (name, source_type, source_url) VALUES (?1, 'm3u_file', 'merged')",
        params![target_name],
    )
    .map_err(|e| e.to_string())?;

    let new_id = conn.last_insert_rowid();

    // Copy channels from all sources, deduplicating by stream_url
    let placeholders: Vec<String> = source_ids.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, is_vod)
         SELECT ?1, name, group_name, stream_url, logo_url, epg_id, is_vod
         FROM channels WHERE playlist_id IN ({})
         GROUP BY stream_url",
        placeholders.join(", ")
    );

    let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    all_params.push(Box::new(new_id));
    for id in &source_ids {
        all_params.push(Box::new(*id));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = all_params.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
        .map_err(|e| e.to_string())?;

    let playlist = conn
        .query_row(
            "SELECT id, name, source_type, source_url, xtream_username, xtream_password,
                    auto_update_interval, last_updated_at, created_at
             FROM playlists WHERE id = ?1",
            params![new_id],
            |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source_type: row.get(2)?,
                    source_url: row.get(3)?,
                    xtream_username: row.get(4)?,
                    xtream_password: row.get(5)?,
                    auto_update_interval: row.get(6)?,
                    last_updated_at: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(playlist)
}

#[tauri::command]
pub async fn split_playlist(
    playlist_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<Playlist>, String> {
    let conn = db.conn.lock().unwrap();

    // Get distinct groups
    let mut stmt = conn
        .prepare("SELECT DISTINCT group_name FROM channels WHERE playlist_id = ?1")
        .map_err(|e| e.to_string())?;

    let groups: Vec<String> = stmt
        .query_map(params![playlist_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Get original playlist name
    let original_name: String = conn
        .query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let mut new_playlists = Vec::new();

    for group in &groups {
        let playlist_name = if group.is_empty() {
            format!("{} - Ungrouped", original_name)
        } else {
            format!("{} - {}", original_name, group)
        };

        conn.execute(
            "INSERT INTO playlists (name, source_type, source_url) VALUES (?1, 'm3u_file', 'split')",
            params![playlist_name],
        )
        .map_err(|e| e.to_string())?;

        let new_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, is_vod)
             SELECT ?1, name, group_name, stream_url, logo_url, epg_id, is_vod
             FROM channels WHERE playlist_id = ?2 AND group_name = ?3",
            params![new_id, playlist_id, group],
        )
        .map_err(|e| e.to_string())?;

        let playlist = conn
            .query_row(
                "SELECT id, name, source_type, source_url, xtream_username, xtream_password,
                        auto_update_interval, last_updated_at, created_at
                 FROM playlists WHERE id = ?1",
                params![new_id],
                |row| {
                    Ok(Playlist {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        source_type: row.get(2)?,
                        source_url: row.get(3)?,
                        xtream_username: row.get(4)?,
                        xtream_password: row.get(5)?,
                        auto_update_interval: row.get(6)?,
                        last_updated_at: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        new_playlists.push(playlist);
    }

    Ok(new_playlists)
}

#[tauri::command]
pub async fn export_playlist(
    playlist_id: i64,
    output_path: String,
    db: State<'_, Database>,
) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT name, group_name, stream_url, logo_url, epg_id
             FROM channels WHERE playlist_id = ?1 ORDER BY group_name, name",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, String, Option<String>, Option<String>)> = stmt
        .query_map(params![playlist_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut output = String::from("#EXTM3U\n");
    for (name, group, url, logo, epg_id) in &rows {
        let mut attrs = Vec::new();
        if let Some(id) = epg_id {
            attrs.push(format!("tvg-id=\"{}\"", id));
        }
        if let Some(logo_url) = logo {
            attrs.push(format!("tvg-logo=\"{}\"", logo_url));
        }
        if !group.is_empty() {
            attrs.push(format!("group-title=\"{}\"", group));
        }

        let attr_str = if attrs.is_empty() {
            String::new()
        } else {
            format!(" {}", attrs.join(" "))
        };

        output.push_str(&format!("#EXTINF:-1{},{}\n{}\n", attr_str, name, url));
    }

    std::fs::write(&output_path, output).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}
