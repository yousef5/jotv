use crate::db::models::Download;
use crate::db::Database;
use futures_util::StreamExt;
use rusqlite::params;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as TokioMutex;
use tokio_util::sync::CancellationToken;

/// Manages active download tasks
pub struct DownloadManager {
    active: Arc<TokioMutex<HashMap<i64, CancellationToken>>>,
}

impl DownloadManager {
    pub fn new() -> Self {
        DownloadManager {
            active: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    id: i64,
    downloaded_bytes: i64,
    total_bytes: Option<i64>,
    progress: f64,
    status: String,
    speed_bps: i64,
}

fn get_download_dir(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().unwrap_or_default().join("downloads");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' ' { c } else { '_' })
        .collect::<String>()
        .trim()
        .to_string()
}

fn query_download(conn: &rusqlite::Connection, id: i64) -> Result<Download, String> {
    conn.query_row(
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
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    url: String,
    filename: String,
    channel_id: i64,
    db: State<'_, Database>,
    dm: State<'_, DownloadManager>,
) -> Result<Download, String> {
    let download_dir = get_download_dir(&app);
    let safe_name = sanitize_filename(&filename);
    let file_path = download_dir.join(&safe_name);
    let file_path_str = file_path.to_string_lossy().to_string();

    // Insert download record (channel_id = NULL for episode/direct downloads)
    let ch_id: Option<i64> = if channel_id > 0 { Some(channel_id) } else { None };
    let download_id = {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads (channel_id, url, file_path, status) VALUES (?1, ?2, ?3, 'downloading')",
            params![ch_id, url, file_path_str],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        query_download(&conn, id)?
    };

    let id = download_id.id;
    let token = CancellationToken::new();
    dm.active.lock().await.insert(id, token.clone());

    // Spawn download task
    let app_clone = app.clone();
    let url_clone = url.clone();

    tokio::spawn(async move {
        let db_ref = app_clone.state::<Database>();
        let result = run_download(
            &app_clone, id, &url_clone, &file_path, &db_ref, token,
        ).await;

        let status = match result {
            Ok(()) => "completed",
            Err(ref e) if e.contains("cancelled") => "paused",
            Err(_) => "failed",
        };

        {
            let db_ref = app_clone.state::<Database>();
            let conn = db_ref.conn.lock().unwrap();
            if status == "completed" {
                conn.execute(
                    "UPDATE downloads SET status = 'completed', progress = 1.0, completed_at = datetime('now') WHERE id = ?1",
                    params![id],
                ).ok();
            } else {
                conn.execute(
                    "UPDATE downloads SET status = ?2 WHERE id = ?1",
                    params![id, status],
                ).ok();
            }
        }

        let _ = app_clone.emit("download-progress", DownloadProgress {
            id,
            downloaded_bytes: 0,
            total_bytes: None,
            progress: if status == "completed" { 1.0 } else { -1.0 },
            status: status.to_string(),
            speed_bps: 0,
        });

        app_clone.state::<DownloadManager>().active.lock().await.remove(&id);
    });

    Ok(download_id)
}

async fn run_download(
    app: &AppHandle,
    id: i64,
    url: &str,
    file_path: &PathBuf,
    db: &State<'_, Database>,
    cancel: CancellationToken,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))   // 2 hours for large files
        .connect_timeout(std::time::Duration::from_secs(30))
        .user_agent("JEOTV/1.0")
        .tcp_nodelay(true)                                // disable Nagle for max throughput
        .pool_max_idle_per_host(5)
        .build()
        .map_err(|e| e.to_string())?;

    // Check if we can resume a partial download
    let existing_size = if file_path.exists() {
        std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    // Try the URL directly. If it fails (4xx), try alternative URL formats.
    // Xtream servers use: /series/user/pass/id.ext OR /user/pass/id.ext
    let try_urls: Vec<String> = {
        let mut urls = vec![url.to_string()];
        // If URL has /series/ or /movie/, try without the prefix
        if url.contains("/series/") {
            urls.push(url.replacen("/series/", "/", 1));
        }
        if url.contains("/movie/") {
            urls.push(url.replacen("/movie/", "/", 1));
        }
        // Also try the reverse: if no prefix, add /series/
        if !url.contains("/series/") && !url.contains("/movie/") && !url.contains("/live/") {
            // Insert /series/ after server:port/
            if let Some(pos) = url.find("://") {
                if let Some(slash) = url[pos+3..].find('/') {
                    let insert_at = pos + 3 + slash;
                    let mut with_series = url[..insert_at].to_string();
                    with_series.push_str("/series");
                    with_series.push_str(&url[insert_at..]);
                    urls.push(with_series);
                }
            }
        }
        urls
    };

    let mut resp = None;
    let mut last_err = String::new();

    for try_url in &try_urls {
        let mut request = client.get(try_url.as_str());
        if existing_size > 0 {
            request = request.header("Range", format!("bytes={}-", existing_size));
        }

        match request.send().await {
            Ok(r) => {
                if r.status().is_success() || r.status().as_u16() == 206 {
                    resp = Some(r);
                    break;
                }
                last_err = format!("HTTP {} from {}", r.status().as_u16(), try_url);
            }
            Err(e) => {
                last_err = format!("{}: {}", try_url, e);
            }
        }
    }

    let resp = resp.ok_or_else(|| format!("Download failed: {}", last_err))?;

    let total_bytes = if existing_size > 0 && resp.status() == 206 {
        resp.content_length().map(|cl| cl as i64 + existing_size as i64)
    } else {
        resp.content_length().map(|cl| cl as i64)
    };

    let resuming = existing_size > 0 && resp.status() == 206;

    // Update DB with total size
    if let Ok(conn) = db.conn.lock() {
        conn.execute(
            "UPDATE downloads SET total_bytes = ?2 WHERE id = ?1",
            params![id, total_bytes],
        ).ok();
    }

    // Open file for writing (append if resuming)
    let file = if resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(file_path)
            .await
            .map_err(|e| e.to_string())?
    } else {
        tokio::fs::File::create(file_path)
            .await
            .map_err(|e| e.to_string())?
    };

    let mut writer = tokio::io::BufWriter::with_capacity(1024 * 1024, file); // 1MB write buffer
    let mut stream = resp.bytes_stream();
    let mut downloaded: i64 = if resuming { existing_size as i64 } else { 0 };
    let mut last_emit = std::time::Instant::now();
    let mut last_bytes = downloaded;
    let start_time = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                writer.flush().await.ok();
                return Err("cancelled".to_string());
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        writer.write_all(&bytes).await.map_err(|e| e.to_string())?;
                        downloaded += bytes.len() as i64;

                        // Emit progress every 500ms
                        let now = std::time::Instant::now();
                        if now.duration_since(last_emit).as_millis() >= 500 {
                            let elapsed = now.duration_since(start_time).as_secs_f64();
                            let speed = if elapsed > 0.0 { ((downloaded - last_bytes) as f64 / now.duration_since(last_emit).as_secs_f64()) as i64 } else { 0 };
                            let progress = total_bytes.map(|t| if t > 0 { downloaded as f64 / t as f64 } else { 0.0 }).unwrap_or(0.0);

                            let _ = app.emit("download-progress", DownloadProgress {
                                id, downloaded_bytes: downloaded, total_bytes,
                                progress, status: "downloading".to_string(), speed_bps: speed,
                            });

                            // Update DB periodically
                            if let Ok(conn) = db.conn.lock() {
                                conn.execute(
                                    "UPDATE downloads SET downloaded_bytes = ?2, progress = ?3 WHERE id = ?1",
                                    params![id, downloaded, progress],
                                ).ok();
                            }

                            last_emit = now;
                            last_bytes = downloaded;
                        }
                    }
                    Some(Err(e)) => {
                        writer.flush().await.ok();
                        return Err(e.to_string());
                    }
                    None => {
                        // Stream finished
                        writer.flush().await.ok();
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[tauri::command]
pub async fn queue_download(
    channel_id: i64,
    url: String,
    db: State<'_, Database>,
) -> Result<Download, String> {
    let ch_id: Option<i64> = if channel_id > 0 { Some(channel_id) } else { None };
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO downloads (channel_id, url, status) VALUES (?1, ?2, 'queued')",
        params![ch_id, url],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    query_download(&conn, id)
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
pub async fn pause_download(
    id: i64,
    db: State<'_, Database>,
    dm: State<'_, DownloadManager>,
) -> Result<(), String> {
    // Cancel the active task
    if let Some(token) = dm.active.lock().await.get(&id) {
        token.cancel();
    }
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE downloads SET status = 'paused' WHERE id = ?1 AND status IN ('queued', 'downloading')",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn resume_download(
    app: AppHandle,
    id: i64,
    db: State<'_, Database>,
    dm: State<'_, DownloadManager>,
) -> Result<(), String> {
    let (url, file_path, _channel_id) = {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = 'downloading' WHERE id = ?1 AND status = 'paused'",
            params![id],
        )
        .map_err(|e| e.to_string())?;

        let row = conn.query_row(
            "SELECT url, file_path, channel_id FROM downloads WHERE id = ?1",
            params![id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
        ).map_err(|e| e.to_string())?;
        row
    };

    let file_path = match file_path {
        Some(fp) => PathBuf::from(fp),
        None => {
            let download_dir = get_download_dir(&app);
            let ext = url.rsplit('.').next().unwrap_or("mp4");
            download_dir.join(format!("download_{}.{}", id, ext))
        }
    };
    let file_path_str = file_path.to_string_lossy().to_string();

    // Update file_path if missing
    {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET file_path = ?2 WHERE id = ?1 AND file_path IS NULL",
            params![id, file_path_str],
        ).ok();
    }

    let token = CancellationToken::new();
    dm.active.lock().await.insert(id, token.clone());

    let app_clone = app.clone();

    tokio::spawn(async move {
        let db_ref = app_clone.state::<Database>();
        let result = run_download(&app_clone, id, &url, &file_path, &db_ref, token).await;
        let status = match result {
            Ok(()) => "completed",
            Err(ref e) if e.contains("cancelled") => "paused",
            Err(_) => "failed",
        };
        {
            let db_ref = app_clone.state::<Database>();
            let conn = db_ref.conn.lock().unwrap();
            if status == "completed" {
                conn.execute(
                    "UPDATE downloads SET status = 'completed', progress = 1.0, completed_at = datetime('now') WHERE id = ?1",
                    params![id],
                ).ok();
            } else {
                conn.execute("UPDATE downloads SET status = ?2 WHERE id = ?1", params![id, status]).ok();
            }
        }
        let _ = app_clone.emit("download-progress", DownloadProgress {
            id, downloaded_bytes: 0, total_bytes: None,
            progress: if status == "completed" { 1.0 } else { -1.0 },
            status: status.to_string(), speed_bps: 0,
        });
        app_clone.state::<DownloadManager>().active.lock().await.remove(&id);
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_download(
    id: i64,
    db: State<'_, Database>,
    dm: State<'_, DownloadManager>,
) -> Result<(), String> {
    // Cancel active task first
    if let Some(token) = dm.active.lock().await.remove(&id) {
        token.cancel();
    }

    let conn = db.conn.lock().unwrap();
    let file_path: Option<String> = conn
        .query_row("SELECT file_path FROM downloads WHERE id = ?1", params![id], |row| row.get(0))
        .ok()
        .flatten();

    conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;

    if let Some(path) = file_path {
        let _ = std::fs::remove_file(path);
    }

    Ok(())
}

#[tauri::command]
pub async fn clear_completed_downloads(db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM downloads WHERE status = 'completed'", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}
