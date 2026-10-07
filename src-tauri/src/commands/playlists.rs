use crate::db::models::{ChannelGroup, Playlist};
use crate::db::Database;
use crate::parsers::m3u;
use crate::parsers::xtream::{self, SeriesDetail, VodDetail, XtreamAccountInfo, XtreamCredentials};
use std::collections::HashMap;
use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

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
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO playlists (name, source_type, source_url) VALUES (?1, 'm3u_url', ?2)",
        params![name, url],
    )
    .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;

    let playlist_id = conn.last_insert_rowid();

    for entry in &entries {
        conn.execute(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                playlist_id,
                entry.name,
                entry.group,
                entry.stream_url,
                entry.logo_url,
                entry.epg_id,
                entry.content_type,
            ],
        )
        .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;
    }

    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;

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
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO playlists (name, source_type, source_url) VALUES (?1, 'm3u_file', ?2)",
        params![name, file_path],
    )
    .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;

    let playlist_id = conn.last_insert_rowid();

    for entry in &entries {
        conn.execute(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                playlist_id,
                entry.name,
                entry.group,
                entry.stream_url,
                entry.logo_url,
                entry.epg_id,
                entry.content_type,
            ],
        )
        .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;
    }

    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;

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

#[derive(Clone, Serialize)]
struct ImportProgress {
    stage: String,
    current: usize,
    total: usize,
    /// Refresh step: "connect" | "download" | "process" | "save" | "done"
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<&'static str>,
}

/// What a refresh changed, for the "library updated" summary.
#[derive(Clone, Serialize)]
struct SyncSummary {
    playlist_id: i64,
    added_live: usize,
    added_vod: usize,
    added_series: usize,
    updated: usize,
    removed: usize,
}

#[tauri::command]
pub async fn add_playlist_from_xtream(
    app: AppHandle,
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

    let app_clone = app.clone();
    let on_progress = Box::new(move |stage: &str, current: usize, total: usize| {
        let _ = app_clone.emit("xtream-import-progress", ImportProgress {
            stage: stage.to_string(),
            current,
            total,
            phase: None,
        });
    });

    let channels = xtream::fetch_xtream_channels(&creds, Some(on_progress)).await?;
    let total = channels.len();

    let _ = app.emit("xtream-import-progress", ImportProgress {
        stage: format!("Saving {} channels to database...", total),
        current: 0,
        total,
        phase: None,
    });

    let conn = db.conn.lock().unwrap();

    // Optimize SQLite for bulk insert
    conn.execute_batch("PRAGMA synchronous = OFF").map_err(|e| e.to_string())?;
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO playlists (name, source_type, source_url, xtream_username, xtream_password)
         VALUES (?1, 'xtream', ?2, ?3, ?4)",
        params![name, server, username, password],
    )
    .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;

    let playlist_id = conn.last_insert_rowid();

    // Batch insert using prepared statement
    {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, added_on_server, rating, year, genre)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        ).map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;

        for (i, ch) in channels.iter().enumerate() {
            stmt.execute(params![
                playlist_id,
                ch.name,
                ch.group,
                ch.stream_url,
                ch.logo_url,
                ch.epg_id,
                ch.content_type,
                ch.added_on_server,
                ch.rating,
                ch.year,
                ch.genre,
            ])
            .map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;

            // Emit progress every 2000 channels
            if (i + 1) % 2000 == 0 || i + 1 == total {
                let _ = app.emit("xtream-import-progress", ImportProgress {
                    stage: format!("Saving channels... {}/{}", i + 1, total),
                    current: i + 1,
                    total,
                    phase: None,
                });
            }
        }
    }

    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA synchronous = NORMAL").map_err(|e| e.to_string())?;

    let _ = app.emit("xtream-import-progress", ImportProgress {
        stage: "Import complete!".to_string(),
        current: total,
        total,
        phase: None,
    });

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
pub async fn get_xtream_account_info(
    server: String,
    username: String,
    password: String,
) -> Result<XtreamAccountInfo, String> {
    let creds = XtreamCredentials { server, username, password };
    xtream::fetch_xtream_account_info(&creds).await
}

#[tauri::command]
pub async fn get_series_info(
    server: String,
    username: String,
    password: String,
    series_id: i64,
) -> Result<SeriesDetail, String> {
    let creds = XtreamCredentials { server, username, password };
    xtream::fetch_series_info(&creds, series_id).await
}

#[tauri::command]
pub async fn get_vod_info(
    server: String,
    username: String,
    password: String,
    vod_id: i64,
) -> Result<VodDetail, String> {
    let creds = XtreamCredentials { server, username, password };
    xtream::fetch_vod_info(&creds, vod_id).await
}

#[tauri::command]
pub async fn refresh_playlist(
    app: AppHandle,
    id: i64,
    db: State<'_, Database>,
) -> Result<Playlist, String> {
    // Read playlist info before the async work
    let (source_type, source_url, xtream_username, xtream_password) = {
        let conn = db.conn.lock().unwrap();
        conn.query_row(
            "SELECT source_type, source_url, xtream_username, xtream_password FROM playlists WHERE id = ?1",
            params![id],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            )),
        ).map_err(|e| format!("Playlist not found: {}", e))?
    };

    let emit = |phase: &'static str, stage: &str, current: usize, total: usize| {
        let _ = app.emit("xtream-import-progress", ImportProgress {
            stage: stage.to_string(), current, total, phase: Some(phase),
        });
    };

    // Fetch new data based on source type
    emit("connect", "Connecting to the server...", 0, 0);

    let channels: Vec<xtream::XtreamChannel>;

    match source_type.as_str() {
        "xtream" => {
            let username = xtream_username.ok_or("Missing xtream username")?;
            let password = xtream_password.ok_or("Missing xtream password")?;
            let creds = XtreamCredentials {
                server: source_url.clone(),
                username,
                password,
            };
            let app_c = app.clone();
            let on_progress = Box::new(move |stage: &str, current: usize, total: usize| {
                let phase = if stage.starts_with("Processing") || stage.starts_with("Fetch complete") { "process" } else { "download" };
                let _ = app_c.emit("xtream-import-progress", ImportProgress {
                    stage: stage.to_string(), current, total, phase: Some(phase),
                });
            });
            channels = xtream::fetch_xtream_channels(&creds, Some(on_progress)).await?;
        }
        "m3u_url" => {
            let response = reqwest::get(&source_url)
                .await
                .map_err(|e| format!("Failed to fetch M3U: {}", e))?;
            let content = response.text().await
                .map_err(|e| format!("Failed to read M3U body: {}", e))?;
            let entries = m3u::parse_m3u(&content);
            channels = entries.into_iter().map(|e| xtream::XtreamChannel {
                name: e.name,
                stream_id: 0,
                stream_url: e.stream_url,
                logo_url: e.logo_url,
                epg_id: e.epg_id,
                group: e.group,
                content_type: e.content_type,
                added_on_server: 0,
                rating: None,
                year: None,
                genre: None,
            }).collect();
        }
        _ => {
            return Err(format!("Cannot refresh source type: {}", source_type));
        }
    }

    let total = channels.len();
    emit("save", &format!("Saving {} channels...", total), 0, total);

    // Sync channels in place (matched by stream URL) so channel IDs stay stable.
    // Favorites and viewing history reference channels with ON DELETE CASCADE,
    // so a delete-and-reinsert would wipe them on every refresh.
    let conn = db.conn.lock().unwrap();
    conn.execute_batch("PRAGMA synchronous = OFF").map_err(|e| e.to_string())?;
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;

    let rollback = |e: rusqlite::Error| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() };
    let mut summary = SyncSummary { playlist_id: id, added_live: 0, added_vod: 0, added_series: 0, updated: 0, removed: 0 };

    let mut existing: HashMap<String, i64> = {
        let mut stmt = conn
            .prepare("SELECT stream_url, id FROM channels WHERE playlist_id = ?1")
            .map_err(rollback)?;
        let rows = stmt
            .query_map(params![id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .map_err(rollback)?;
        rows.collect::<Result<_, _>>().map_err(rollback)?
    };

    {
        let mut update = conn.prepare_cached(
            "UPDATE channels SET name = ?2, group_name = ?3, logo_url = ?4, epg_id = ?5, content_type = ?6,
                    added_on_server = ?7, rating = ?8, year = ?9, genre = ?10
             WHERE id = ?1"
        ).map_err(rollback)?;
        let mut insert = conn.prepare_cached(
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, added_on_server, rating, year, genre)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        ).map_err(rollback)?;

        for (i, ch) in channels.iter().enumerate() {
            match existing.get(&ch.stream_url) {
                Some(_) => summary.updated += 1,
                None => match ch.content_type.as_str() {
                    "vod" => summary.added_vod += 1,
                    "series" => summary.added_series += 1,
                    _ => summary.added_live += 1,
                },
            }
            match existing.remove(&ch.stream_url) {
                Some(channel_id) => update.execute(params![
                    channel_id, ch.name, ch.group, ch.logo_url, ch.epg_id, ch.content_type,
                    ch.added_on_server, ch.rating, ch.year, ch.genre,
                ]),
                None => insert.execute(params![
                    id, ch.name, ch.group, ch.stream_url, ch.logo_url, ch.epg_id, ch.content_type,
                    ch.added_on_server, ch.rating, ch.year, ch.genre,
                ]),
            }
            .map_err(rollback)?;

            if (i + 1) % 2000 == 0 || i + 1 == total {
                let _ = app.emit("xtream-import-progress", ImportProgress {
                    stage: format!("Saving channels... {}/{}", i + 1, total),
                    current: i + 1,
                    total,
                    phase: Some("save"),
                });
            }
        }

        // Whatever is left no longer exists on the server
        summary.removed = existing.len();
        let mut delete = conn.prepare_cached("DELETE FROM channels WHERE id = ?1").map_err(rollback)?;
        for channel_id in existing.values() {
            delete.execute(params![channel_id]).map_err(rollback)?;
        }
    }

    // Update last_updated_at
    conn.execute(
        "UPDATE playlists SET last_updated_at = datetime('now') WHERE id = ?1",
        params![id],
    ).map_err(|e| { let _ = conn.execute_batch("ROLLBACK"); e.to_string() })?;

    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA synchronous = NORMAL").map_err(|e| e.to_string())?;

    emit("done", "Library updated", total, total);
    let _ = app.emit("library-sync-summary", summary);

    let playlist = conn
        .query_row(
            "SELECT id, name, source_type, source_url, xtream_username, xtream_password,
                    auto_update_interval, last_updated_at, created_at
             FROM playlists WHERE id = ?1",
            params![id],
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
        "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type)
         SELECT ?1, name, group_name, stream_url, logo_url, epg_id, content_type
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
            "INSERT INTO channels (playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type)
             SELECT ?1, name, group_name, stream_url, logo_url, epg_id, content_type
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
