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
    eta_seconds: Option<i64>,
}

fn get_download_dir(app: &AppHandle) -> PathBuf {
    // Check if user set a custom download directory
    let custom_dir = app
        .try_state::<Database>()
        .and_then(|db| {
            let conn = db.conn.lock().ok()?;
            conn.query_row(
                "SELECT value FROM settings WHERE key = 'download_dir'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .filter(|s| !s.is_empty());

    let dir = match custom_dir {
        Some(path) => PathBuf::from(path),
        None => app.path().app_data_dir().unwrap_or_default().join("downloads"),
    };
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[tauri::command]
pub async fn get_default_download_dir(app: AppHandle) -> Result<String, String> {
    let dir = app.path().app_data_dir().unwrap_or_default().join("downloads");
    Ok(dir.to_string_lossy().to_string())
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
        "SELECT d.id, d.channel_id, d.url, d.file_path, d.status, d.progress, d.total_bytes,
                d.downloaded_bytes, d.retry_count, d.created_at, d.completed_at,
                c.name as channel_name, c.logo_url as channel_logo
         FROM downloads d
         LEFT JOIN channels c ON d.channel_id = c.id
         WHERE d.id = ?1",
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
                channel_name: row.get(11)?,
                channel_logo: row.get(12)?,
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

        let (final_downloaded, final_total) = {
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
            // Read actual byte counts for the final event
            conn.query_row(
                "SELECT downloaded_bytes, total_bytes FROM downloads WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, i64>(0).unwrap_or(0), row.get::<_, Option<i64>>(1).unwrap_or(None))),
            ).unwrap_or((0, None))
        };

        let _ = app_clone.emit("download-progress", DownloadProgress {
            id,
            downloaded_bytes: final_downloaded,
            total_bytes: final_total,
            progress: if status == "completed" { 1.0 } else { -1.0 },
            status: status.to_string(),
            speed_bps: 0,
            eta_seconds: None,
        });

        app_clone.state::<DownloadManager>().active.lock().await.remove(&id);
    });

    Ok(download_id)
}

/// Resolve the working URL from a list of fallback URL formats
async fn resolve_download_url(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let mut try_urls: Vec<String> = vec![url.to_string()];
    if url.contains("/series/") {
        try_urls.push(url.replacen("/series/", "/", 1));
    }
    if url.contains("/movie/") {
        try_urls.push(url.replacen("/movie/", "/", 1));
    }
    if !url.contains("/series/") && !url.contains("/movie/") && !url.contains("/live/") {
        if let Some(pos) = url.find("://") {
            if let Some(slash) = url[pos+3..].find('/') {
                let insert_at = pos + 3 + slash;
                let mut with_series = url[..insert_at].to_string();
                with_series.push_str("/series");
                with_series.push_str(&url[insert_at..]);
                try_urls.push(with_series);
            }
        }
    }

    let mut last_err = String::new();
    for try_url in &try_urls {
        match client.head(try_url.as_str()).send().await {
            Ok(r) if r.status().is_success() || r.status().as_u16() == 206 => {
                return Ok(try_url.clone());
            }
            Ok(_) | Err(_) => {}
        }
        // HEAD may not be supported, try GET
        match client.get(try_url.as_str()).header("Range", "bytes=0-0").send().await {
            Ok(r) if r.status().is_success() || r.status().as_u16() == 206 => {
                return Ok(try_url.clone());
            }
            Ok(r) => last_err = format!("HTTP {} from {}", r.status().as_u16(), try_url),
            Err(e) => last_err = format!("{}: {}", try_url, e),
        }
    }
    Err(format!("Download failed: {}", last_err))
}

/// Check if server supports range requests and get total file size
async fn probe_download(client: &reqwest::Client, url: &str) -> (Option<i64>, bool) {
    if let Ok(resp) = client.head(url).send().await {
        let accepts_ranges = resp.headers()
            .get("accept-ranges")
            .map(|v| v.to_str().unwrap_or("") != "none")
            .unwrap_or(false);
        let content_length = resp.content_length().map(|cl| cl as i64);
        if accepts_ranges && content_length.is_some() {
            return (content_length, true);
        }
        // Some servers don't advertise accept-ranges but support it
        if let Some(total) = content_length {
            if let Ok(r) = client.get(url).header("Range", "bytes=0-0").send().await {
                if r.status().as_u16() == 206 {
                    return (Some(total), true);
                }
            }
            return (Some(total), false);
        }
    }
    (None, false)
}

const NUM_CHUNKS: i64 = 8;
const MIN_CHUNK_SIZE: i64 = 2 * 1024 * 1024; // 2MB minimum per chunk

async fn run_download(
    app: &AppHandle,
    id: i64,
    url: &str,
    file_path: &PathBuf,
    db: &State<'_, Database>,
    cancel: CancellationToken,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .connect_timeout(std::time::Duration::from_secs(30))
        .user_agent("JEOTV/1.0")
        .tcp_nodelay(true)
        .pool_max_idle_per_host(10)
        .build()
        .map_err(|e| e.to_string())?;

    let resolved_url = resolve_download_url(&client, url).await?;

    // Check if we can resume a partial download
    let existing_size = if file_path.exists() {
        std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let (total_bytes_opt, supports_range) = probe_download(&client, &resolved_url).await;

    // Update DB with total size
    if let Ok(conn) = db.conn.lock() {
        conn.execute(
            "UPDATE downloads SET total_bytes = ?2 WHERE id = ?1",
            params![id, total_bytes_opt],
        ).ok();
    }

    // Decide: multi-chunk or single-stream
    let use_multi = supports_range
        && total_bytes_opt.map(|t| t > MIN_CHUNK_SIZE * 2).unwrap_or(false)
        && existing_size == 0; // don't multi-chunk on resume

    if use_multi {
        run_multi_chunk_download(app, id, &client, &resolved_url, file_path, db, cancel, total_bytes_opt.unwrap()).await
    } else {
        run_single_stream_download(app, id, &client, &resolved_url, file_path, db, cancel, existing_size, total_bytes_opt).await
    }
}

/// Multi-connection chunked download for maximum speed
async fn run_multi_chunk_download(
    app: &AppHandle,
    id: i64,
    client: &reqwest::Client,
    url: &str,
    file_path: &PathBuf,
    _db: &State<'_, Database>,
    cancel: CancellationToken,
    total_bytes: i64,
) -> Result<(), String> {
    let chunk_size = std::cmp::max(total_bytes / NUM_CHUNKS, MIN_CHUNK_SIZE);
    let mut ranges: Vec<(i64, i64)> = Vec::new();
    let mut start = 0i64;
    while start < total_bytes {
        let end = std::cmp::min(start + chunk_size - 1, total_bytes - 1);
        ranges.push((start, end));
        start = end + 1;
    }

    let num_parts = ranges.len();
    let downloaded_counter = Arc::new(std::sync::atomic::AtomicI64::new(0));

    // Pre-create the output file
    {
        tokio::fs::File::create(file_path).await.map_err(|e| e.to_string())?;
    }

    // Progress reporter task
    let app_progress = app.clone();
    let cancel_progress = cancel.clone();
    let counter_progress = downloaded_counter.clone();
    let db_id = id;
    let total = total_bytes;
    let app_for_db = app.clone();
    let progress_handle = tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        let mut last_bytes = 0i64;
        let mut last_time = start_time;
        loop {
            tokio::select! {
                _ = cancel_progress.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                    let now = std::time::Instant::now();
                    let dl = counter_progress.load(std::sync::atomic::Ordering::Relaxed);
                    let interval = now.duration_since(last_time).as_secs_f64();
                    let speed = if interval > 0.0 { ((dl - last_bytes) as f64 / interval) as i64 } else { 0 };
                    let progress = if total > 0 { dl as f64 / total as f64 } else { 0.0 };
                    let eta = if speed > 0 { Some((total - dl) / speed) } else { None };

                    let _ = app_progress.emit("download-progress", DownloadProgress {
                        id: db_id,
                        downloaded_bytes: dl,
                        total_bytes: Some(total),
                        progress,
                        status: "downloading".to_string(),
                        speed_bps: speed,
                        eta_seconds: eta,
                    });

                    if let Ok(conn) = app_for_db.state::<Database>().conn.lock() {
                        conn.execute(
                            "UPDATE downloads SET downloaded_bytes = ?2, progress = ?3 WHERE id = ?1",
                            params![db_id, dl, progress],
                        ).ok();
                    }

                    last_bytes = dl;
                    last_time = now;

                    if dl >= total { break; }
                }
            }
        }
    });

    // Spawn chunk download tasks
    let mut handles = Vec::with_capacity(num_parts);
    for (range_start, range_end) in ranges {
        let client = client.clone();
        let url = url.to_string();
        let path = file_path.clone();
        let counter = downloaded_counter.clone();
        let cancel_chunk = cancel.clone();

        handles.push(tokio::spawn(async move {
            let max_retries: u32 = 3;
            let mut chunk_downloaded: i64 = 0;

            for attempt in 0..=max_retries {
                if attempt > 0 {
                    // Backoff: 1s, 2s, 4s
                    tokio::time::sleep(std::time::Duration::from_secs(1 << (attempt - 1))).await;
                    // Subtract previously counted bytes for this chunk so we re-count from the chunk offset
                    if chunk_downloaded > 0 {
                        counter.fetch_sub(chunk_downloaded, std::sync::atomic::Ordering::Relaxed);
                        chunk_downloaded = 0;
                    }
                }

                if cancel_chunk.is_cancelled() {
                    return Err("cancelled".to_string());
                }

                let resp = match client
                    .get(&url)
                    .header("Range", format!("bytes={}-{}", range_start, range_end))
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        if attempt == max_retries {
                            return Err(e.to_string());
                        }
                        continue;
                    }
                };

                if !resp.status().is_success() && resp.status().as_u16() != 206 {
                    if attempt == max_retries {
                        return Err(format!("HTTP {}", resp.status().as_u16()));
                    }
                    continue;
                }

                let mut stream = resp.bytes_stream();
                let file = tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .await
                    .map_err(|e| e.to_string())?;

                let mut writer = tokio::io::BufWriter::with_capacity(512 * 1024, file);
                use tokio::io::AsyncSeekExt;
                writer.seek(std::io::SeekFrom::Start(range_start as u64)).await.map_err(|e| e.to_string())?;
                chunk_downloaded = 0;

                loop {
                    tokio::select! {
                        _ = cancel_chunk.cancelled() => {
                            writer.flush().await.ok();
                            return Err("cancelled".to_string());
                        }
                        chunk = stream.next() => {
                            match chunk {
                                Some(Ok(bytes)) => {
                                    writer.write_all(&bytes).await.map_err(|e| e.to_string())?;
                                    let len = bytes.len() as i64;
                                    chunk_downloaded += len;
                                    counter.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
                                }
                                Some(Err(_e)) => {
                                    writer.flush().await.ok();
                                    break; // Stream error, exit inner loop to retry
                                }
                                None => {
                                    writer.flush().await.ok();
                                    return Ok(()); // Download complete for this chunk
                                }
                            }
                        }
                    }
                }

                // If we broke out of the loop, the stream had an error
                if attempt == max_retries {
                    return Err("chunk failed after retries".to_string());
                }
                // Retries remaining => continue to next attempt
            }
            Err("chunk failed after retries".to_string())
        }));
    }

    // Wait for all chunks
    let mut any_error = None;
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if any_error.is_none() {
                    any_error = Some(e);
                    cancel.cancel();
                }
            }
            Err(e) => {
                if any_error.is_none() {
                    any_error = Some(e.to_string());
                    cancel.cancel();
                }
            }
        }
    }

    progress_handle.abort();

    match any_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Single-stream download (fallback when range not supported, or for resume)
async fn run_single_stream_download(
    app: &AppHandle,
    id: i64,
    client: &reqwest::Client,
    url: &str,
    file_path: &PathBuf,
    db: &State<'_, Database>,
    cancel: CancellationToken,
    existing_size: u64,
    total_bytes: Option<i64>,
) -> Result<(), String> {
    let mut request = client.get(url);
    if existing_size > 0 {
        request = request.header("Range", format!("bytes={}-", existing_size));
    }

    let resp = request.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }

    let total_bytes = if existing_size > 0 && resp.status().as_u16() == 206 {
        resp.content_length().map(|cl| cl as i64 + existing_size as i64).or(total_bytes)
    } else {
        resp.content_length().map(|cl| cl as i64).or(total_bytes)
    };

    let resuming = existing_size > 0 && resp.status().as_u16() == 206;

    if let Ok(conn) = db.conn.lock() {
        conn.execute(
            "UPDATE downloads SET total_bytes = ?2 WHERE id = ?1",
            params![id, total_bytes],
        ).ok();
    }

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

    let mut writer = tokio::io::BufWriter::with_capacity(1024 * 1024, file);
    let mut stream = resp.bytes_stream();
    let mut downloaded: i64 = if resuming { existing_size as i64 } else { 0 };
    let mut last_emit = std::time::Instant::now();
    let mut last_bytes = downloaded;
    let mut stream_retries: u32 = 0;
    let max_stream_retries: u32 = 3;

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
                        stream_retries = 0; // Reset on successful data

                        let now = std::time::Instant::now();
                        if now.duration_since(last_emit).as_millis() >= 250 {
                            let interval = now.duration_since(last_emit).as_secs_f64();
                            let speed = if interval > 0.0 { ((downloaded - last_bytes) as f64 / interval) as i64 } else { 0 };
                            let progress = total_bytes.map(|t| if t > 0 { downloaded as f64 / t as f64 } else { 0.0 }).unwrap_or(0.0);
                            let eta = if speed > 0 {
                                total_bytes.map(|t| (t - downloaded) / speed)
                            } else {
                                None
                            };

                            let _ = app.emit("download-progress", DownloadProgress {
                                id, downloaded_bytes: downloaded, total_bytes,
                                progress, status: "downloading".to_string(), speed_bps: speed,
                                eta_seconds: eta,
                            });

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
                    Some(Err(_e)) => {
                        stream_retries += 1;
                        if stream_retries > max_stream_retries {
                            writer.flush().await.ok();
                            return Err("stream failed after retries".to_string());
                        }
                        // Flush current data, wait with backoff, then resume
                        writer.flush().await.ok();
                        tokio::time::sleep(std::time::Duration::from_secs(1 << (stream_retries - 1))).await;

                        if cancel.is_cancelled() {
                            return Err("cancelled".to_string());
                        }

                        // Re-open file in append mode and create a new stream from where we left off
                        let resume_file = tokio::fs::OpenOptions::new()
                            .append(true)
                            .open(file_path)
                            .await
                            .map_err(|e| e.to_string())?;
                        writer = tokio::io::BufWriter::with_capacity(1024 * 1024, resume_file);

                        let resume_resp = client
                            .get(url)
                            .header("Range", format!("bytes={}-", downloaded))
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;

                        if !resume_resp.status().is_success() && resume_resp.status().as_u16() != 206 {
                            return Err(format!("HTTP {} on resume", resume_resp.status().as_u16()));
                        }

                        stream = resume_resp.bytes_stream();
                    }
                    None => {
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
            "SELECT d.id, d.channel_id, d.url, d.file_path, d.status, d.progress, d.total_bytes,
                    d.downloaded_bytes, d.retry_count, d.created_at, d.completed_at,
                    c.name as channel_name, c.logo_url as channel_logo
             FROM downloads d
             LEFT JOIN channels c ON d.channel_id = c.id
             ORDER BY d.created_at DESC",
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
                channel_name: row.get(11)?,
                channel_logo: row.get(12)?,
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
        let (final_downloaded, final_total) = {
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
            conn.query_row(
                "SELECT downloaded_bytes, total_bytes FROM downloads WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, i64>(0).unwrap_or(0), row.get::<_, Option<i64>>(1).unwrap_or(None))),
            ).unwrap_or((0, None))
        };
        let _ = app_clone.emit("download-progress", DownloadProgress {
            id, downloaded_bytes: final_downloaded, total_bytes: final_total,
            progress: if status == "completed" { 1.0 } else { -1.0 },
            status: status.to_string(), speed_bps: 0, eta_seconds: None,
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

#[tauri::command]
pub async fn open_download_file(file_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err("File not found".to_string());
    }
    open::that(&file_path).map_err(|e| format!("Failed to open file: {}", e))
}

#[tauri::command]
pub async fn show_in_folder(file_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&file_path);
    let folder = if path.is_dir() {
        path.to_path_buf()
    } else if path.exists() {
        path.parent().unwrap_or(path).to_path_buf()
    } else {
        // File doesn't exist, try opening parent anyway
        let parent = path.parent().unwrap_or(path);
        if parent.exists() {
            parent.to_path_buf()
        } else {
            return Err("Folder not found".to_string());
        }
    };
    open::that(folder.to_string_lossy().as_ref())
        .map_err(|e| format!("Failed to open folder: {}", e))
}
