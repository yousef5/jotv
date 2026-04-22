use crate::db::Database;
use regex::Regex;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;

/// Manages active social download processes
pub struct SocialDownloadManager {
    active: Arc<TokioMutex<HashMap<String, tokio::sync::watch::Sender<bool>>>>,
}

impl SocialDownloadManager {
    pub fn new() -> Self {
        SocialDownloadManager {
            active: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct SocialVideoInfo {
    pub id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub view_count: Option<i64>,
    pub formats: Vec<SocialFormat>,
    pub url: String,
    pub platform: String,
}

#[derive(Clone, Serialize)]
pub struct SocialFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub filesize: Option<i64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub label: String,
}

#[derive(Clone, Serialize)]
pub struct SocialDownloadProgress {
    pub download_id: String,
    pub progress: f64,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub status: String,
    pub filename: Option<String>,
}

fn detect_platform(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        "YouTube".into()
    } else if lower.contains("instagram.com") {
        "Instagram".into()
    } else if lower.contains("facebook.com") || lower.contains("fb.watch") {
        "Facebook".into()
    } else if lower.contains("tiktok.com") {
        "TikTok".into()
    } else if lower.contains("twitter.com") || lower.contains("x.com") {
        "Twitter/X".into()
    } else if lower.contains("reddit.com") {
        "Reddit".into()
    } else if lower.contains("twitch.tv") {
        "Twitch".into()
    } else if lower.contains("vimeo.com") {
        "Vimeo".into()
    } else if lower.contains("dailymotion.com") {
        "Dailymotion".into()
    } else if lower.contains("soundcloud.com") {
        "SoundCloud".into()
    } else {
        "Other".into()
    }
}

/// Try to find the yt-dlp binary on the system
async fn find_ytdlp() -> Result<String, String> {
    // Try the plain command first (relies on PATH)
    let paths_to_try = [
        "yt-dlp",
        "/usr/bin/yt-dlp",
        "/usr/local/bin/yt-dlp",
    ];

    // Also try ~/.local/bin/yt-dlp
    let home_path = std::env::var("HOME")
        .map(|h| format!("{}/.local/bin/yt-dlp", h))
        .unwrap_or_default();

    for path in paths_to_try.iter().chain(std::iter::once(&home_path.as_str())) {
        if path.is_empty() {
            continue;
        }
        let result = Command::new(path)
            .arg("--version")
            .output()
            .await;
        if let Ok(output) = result {
            if output.status.success() {
                return Ok(path.to_string());
            }
        }
    }

    Err("yt-dlp not found. Please install it: https://github.com/yt-dlp/yt-dlp#installation".to_string())
}

#[tauri::command]
pub async fn check_ytdlp() -> Result<String, String> {
    let bin = find_ytdlp().await?;
    let output = Command::new(&bin)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(version)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("yt-dlp error: {}", stderr.trim()))
    }
}

/// Helper to parse the raw yt-dlp JSON into SocialVideoInfo
fn parse_video_info(json: &serde_json::Value, url: &str) -> Result<SocialVideoInfo, String> {
    let id = json["id"].as_str().unwrap_or("unknown").to_string();
    let title = json["title"].as_str().unwrap_or("Untitled").to_string();
    let thumbnail = json["thumbnail"].as_str().map(|s| s.to_string());
    let duration = json["duration"].as_f64();
    let uploader = json["uploader"].as_str().map(|s| s.to_string());
    let view_count = json["view_count"].as_i64();
    let platform = detect_platform(url);

    let mut formats = Vec::new();

    // Always add "Best Quality" as the first option
    formats.push(SocialFormat {
        format_id: "bestvideo+bestaudio/best".to_string(),
        ext: "mp4".to_string(),
        resolution: None,
        filesize: None,
        vcodec: None,
        acodec: None,
        label: "Best Quality".to_string(),
    });

    // Parse available formats and group by resolution
    if let Some(raw_formats) = json["formats"].as_array() {
        // Collect formats with both video and audio, grouped by resolution height
        let mut resolution_map: HashMap<i64, Vec<&serde_json::Value>> = HashMap::new();

        for fmt in raw_formats {
            let vcodec = fmt["vcodec"].as_str().unwrap_or("none");
            let height = fmt["height"].as_i64().unwrap_or(0);

            // Only include formats with video
            if vcodec != "none" && height > 0 {
                resolution_map.entry(height).or_default().push(fmt);
            }
        }

        // Sort resolutions descending
        let mut heights: Vec<i64> = resolution_map.keys().copied().collect();
        heights.sort_unstable_by(|a, b| b.cmp(a));

        // For common resolutions, pick the best format
        let target_resolutions = [1080, 720, 480, 360];
        let mut added_resolutions = std::collections::HashSet::new();

        for &target in &target_resolutions {
            // Find the closest matching resolution
            let closest = heights.iter().copied().min_by_key(|&h| (h - target).abs());

            if let Some(height) = closest {
                // Only add if we haven't added a very similar resolution
                if added_resolutions.contains(&height) {
                    continue;
                }
                if let Some(candidates) = resolution_map.get(&height) {
                    // Prefer formats with both audio and video, then by filesize
                    let best = candidates.iter().max_by_key(|f| {
                        let has_audio = f["acodec"].as_str().unwrap_or("none") != "none";
                        let filesize = f["filesize"].as_i64().unwrap_or(0);
                        let is_mp4 = f["ext"].as_str().unwrap_or("") == "mp4";
                        // Score: prefer mp4, prefer has audio, prefer larger file
                        (is_mp4 as i64 * 1_000_000_000)
                            + (has_audio as i64 * 100_000_000)
                            + filesize
                    });

                    if let Some(fmt) = best {
                        let format_id = fmt["format_id"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string();
                        let ext = fmt["ext"]
                            .as_str()
                            .unwrap_or("mp4")
                            .to_string();
                        let resolution = Some(format!("{}p", height));
                        let filesize = fmt["filesize"].as_i64().or(fmt["filesize_approx"].as_i64());
                        let vcodec = fmt["vcodec"].as_str().map(|s| s.to_string());
                        let acodec = fmt["acodec"].as_str().map(|s| s.to_string());
                        let label = format!("{}p {}", height, ext.to_uppercase());

                        formats.push(SocialFormat {
                            format_id,
                            ext,
                            resolution,
                            filesize,
                            vcodec,
                            acodec,
                            label,
                        });
                        added_resolutions.insert(height);
                    }
                }
            }
        }

        // If no resolution-specific formats were found, add any available ones
        if added_resolutions.is_empty() {
            for &height in heights.iter().take(4) {
                if let Some(candidates) = resolution_map.get(&height) {
                    if let Some(fmt) = candidates.first() {
                        let format_id = fmt["format_id"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string();
                        let ext = fmt["ext"].as_str().unwrap_or("mp4").to_string();
                        let filesize =
                            fmt["filesize"].as_i64().or(fmt["filesize_approx"].as_i64());
                        let vcodec = fmt["vcodec"].as_str().map(|s| s.to_string());
                        let acodec = fmt["acodec"].as_str().map(|s| s.to_string());
                        let label = format!("{}p {}", height, ext.to_uppercase());

                        formats.push(SocialFormat {
                            format_id,
                            ext,
                            resolution: Some(format!("{}p", height)),
                            filesize,
                            vcodec,
                            acodec,
                            label,
                        });
                    }
                }
            }
        }
    }

    // Always add "Audio Only" as the last option
    formats.push(SocialFormat {
        format_id: "bestaudio".to_string(),
        ext: "mp3".to_string(),
        resolution: None,
        filesize: None,
        vcodec: None,
        acodec: None,
        label: "Audio Only (MP3)".to_string(),
    });

    Ok(SocialVideoInfo {
        id,
        title,
        thumbnail,
        duration,
        uploader,
        view_count,
        formats,
        url: url.to_string(),
        platform,
    })
}

#[tauri::command]
pub async fn get_social_video_info(url: String) -> Result<SocialVideoInfo, String> {
    let bin = find_ytdlp().await?;

    let output = Command::new(&bin)
        .args(["--dump-json", "--no-download", "--no-warnings", &url])
        .output()
        .await
        .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp error: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse yt-dlp JSON: {}", e))?;

    parse_video_info(&json, &url)
}

/// Parse a size string like "125.30MiB" into bytes
fn parse_size_to_bytes(size_str: &str) -> i64 {
    // Try to parse patterns like "125.30MiB", "1.23GiB", "500.00KiB"
    let re = Regex::new(r"(\d+\.?\d*)\s*(Ki|Mi|Gi|k|M|G)?B?").unwrap();
    if let Some(caps) = re.captures(size_str) {
        let value: f64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0.0);
        let multiplier = match caps.get(2).map(|m| m.as_str()) {
            Some("Ki") | Some("k") => 1024.0,
            Some("Mi") | Some("M") => 1024.0 * 1024.0,
            Some("Gi") | Some("G") => 1024.0 * 1024.0 * 1024.0,
            _ => 1.0,
        };
        (value * multiplier) as i64
    } else {
        0
    }
}

#[tauri::command]
pub async fn start_social_download(
    app: AppHandle,
    url: String,
    format_id: String,
    output_dir: String,
    download_id: String,
    title: String,
    thumbnail: Option<String>,
    platform: String,
    format_label: Option<String>,
    dm: tauri::State<'_, SocialDownloadManager>,
    db: State<'_, Database>,
) -> Result<(), String> {
    let bin = find_ytdlp().await?;

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    // Insert DB record
    {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO social_downloads (id, url, title, thumbnail, platform, format_label, output_dir, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'downloading')",
            params![download_id, url, title, thumbnail, platform, format_label, output_dir],
        ).map_err(|e| e.to_string())?;
    }

    // Create cancellation channel
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    dm.active
        .lock()
        .await
        .insert(download_id.clone(), cancel_tx);

    let active_ref = dm.active.clone();
    let dl_id = download_id.clone();

    tokio::spawn(async move {
        let result =
            run_social_download(&app, &bin, &url, &format_id, &output_dir, &dl_id, &mut cancel_rx)
                .await;

        let status = match &result {
            Ok(_) => "completed",
            Err(e) if e.contains("cancelled") => "cancelled",
            Err(_) => "failed",
        };

        let error_msg = match &result {
            Err(e) if !e.contains("cancelled") => Some(e.clone()),
            _ => None,
        };

        // Update DB with final status
        {
            let db_state = app.state::<Database>();
            let conn = db_state.conn.lock();
            if let Ok(c) = conn {
                if status == "completed" {
                    c.execute(
                        "UPDATE social_downloads SET status = 'completed', progress = 1.0, completed_at = datetime('now') WHERE id = ?1",
                        params![dl_id],
                    ).ok();
                } else {
                    c.execute(
                        "UPDATE social_downloads SET status = ?2 WHERE id = ?1",
                        params![dl_id, status],
                    ).ok();
                }
            }
        }

        let _ = app.emit(
            "social-download-progress",
            SocialDownloadProgress {
                download_id: dl_id.clone(),
                progress: if status == "completed" { 1.0 } else { -1.0 },
                downloaded_bytes: 0,
                total_bytes: None,
                speed: None,
                eta: None,
                status: if let Some(err) = error_msg {
                    format!("failed: {}", err)
                } else {
                    status.to_string()
                },
                filename: None,
            },
        );

        active_ref.lock().await.remove(&dl_id);
    });

    Ok(())
}

async fn run_social_download(
    app: &AppHandle,
    bin: &str,
    url: &str,
    format_id: &str,
    output_dir: &str,
    download_id: &str,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let output_template = format!("{}/%(title)s.%(ext)s", output_dir);

    let mut args = vec![
        "--newline".to_string(),
        "--progress".to_string(),
        "--no-warnings".to_string(),
        // Speed optimizations
        "--concurrent-fragments".to_string(), "16".to_string(),  // 16 parallel fragment downloads
        "--buffer-size".to_string(), "16M".to_string(),          // 16MB download buffer
        "--http-chunk-size".to_string(), "10M".to_string(),      // 10MB chunks
        "--retries".to_string(), "10".to_string(),               // retry failed fragments
        "--fragment-retries".to_string(), "10".to_string(),
        "--no-part".to_string(),                                 // write directly, no .part files
        "-f".to_string(),
        format_id.to_string(),
        "-o".to_string(),
        output_template,
    ];

    // For audio-only downloads, extract audio as mp3
    if format_id == "bestaudio" {
        args.push("--extract-audio".to_string());
        args.push("--audio-format".to_string());
        args.push("mp3".to_string());
    }

    args.push(url.to_string());

    let mut child = Command::new(bin)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn yt-dlp: {}", e))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture stderr".to_string())?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let progress_re =
        Regex::new(r"(\d+\.?\d*)%\s+of\s+~?(\d+\.?\d*)(Ki|Mi|Gi)?B(?:\s+at\s+(\S+)\s+ETA\s+(\S+))?")
            .unwrap();
    let destination_re = Regex::new(r"\[download\]\s+Destination:\s+(.+)").unwrap();
    let merger_re = Regex::new(r"\[Merger\]").unwrap();
    let already_downloaded_re =
        Regex::new(r"\[download\]\s+(.+)\s+has already been downloaded").unwrap();

    let app_clone = app.clone();
    let dl_id = download_id.to_string();

    // Read both stdout and stderr
    let mut stderr_reader = BufReader::new(stderr).lines();
    let mut stdout_reader = BufReader::new(stdout).lines();

    let mut current_filename: Option<String> = None;
    let mut last_progress = SocialDownloadProgress {
        download_id: dl_id.clone(),
        progress: 0.0,
        downloaded_bytes: 0,
        total_bytes: None,
        speed: None,
        eta: None,
        status: "downloading".to_string(),
        filename: None,
    };
    // Track download passes (bestvideo+bestaudio = 2 passes + merge)
    let is_multi_stream = format_id.contains('+');
    let mut download_pass: u32 = 0;
    let mut last_raw_percent: f64 = 0.0;

    loop {
        tokio::select! {
            // Check cancellation
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    child.kill().await.ok();
                    return Err("cancelled".to_string());
                }
            }
            // Read stderr lines (yt-dlp outputs progress to stderr with --newline)
            line = stderr_reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        parse_progress_line(
                            &line,
                            &progress_re,
                            &destination_re,
                            &merger_re,
                            &already_downloaded_re,
                            &mut current_filename,
                            &mut last_progress,
                            &app_clone,
                            &dl_id,
                            is_multi_stream,
                            &mut download_pass,
                            &mut last_raw_percent,
                        );
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            // Read stdout lines as well (some yt-dlp versions output here)
            line = stdout_reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        parse_progress_line(
                            &line,
                            &progress_re,
                            &destination_re,
                            &merger_re,
                            &already_downloaded_re,
                            &mut current_filename,
                            &mut last_progress,
                            &app_clone,
                            &dl_id,
                            is_multi_stream,
                            &mut download_pass,
                            &mut last_raw_percent,
                        );
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    // Wait for the process to complete
    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for yt-dlp: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "yt-dlp exited with code {}",
            status.code().unwrap_or(-1)
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_progress_line(
    line: &str,
    progress_re: &Regex,
    destination_re: &Regex,
    merger_re: &Regex,
    already_downloaded_re: &Regex,
    current_filename: &mut Option<String>,
    last_progress: &mut SocialDownloadProgress,
    app: &AppHandle,
    download_id: &str,
    is_multi_stream: bool,
    download_pass: &mut u32,
    last_raw_percent: &mut f64,
) {
    // Check for destination line — a new destination means a new download pass
    if let Some(caps) = destination_re.captures(line) {
        let filename = caps.get(1).map(|m| m.as_str().trim().to_string());
        *current_filename = filename.clone();
        last_progress.filename = filename.clone();
        // Save file_path to DB
        if let Some(ref fp) = filename {
            let db_state = app.state::<Database>();
            let conn = db_state.conn.lock();
            if let Ok(c) = conn {
                c.execute(
                    "UPDATE social_downloads SET file_path = ?2 WHERE id = ?1",
                    params![download_id, fp],
                ).ok();
            }
        }
        // Detect new pass: when we see a new destination after progress was high
        if *last_raw_percent > 50.0 {
            *download_pass += 1;
            *last_raw_percent = 0.0;
        }
    }

    // Check for already downloaded
    if let Some(caps) = already_downloaded_re.captures(line) {
        let filename = caps.get(1).map(|m| m.as_str().trim().to_string());
        *current_filename = filename.clone();
        last_progress.filename = filename;
        last_progress.progress = 1.0;
        last_progress.status = "completed".to_string();
        last_progress.download_id = download_id.to_string();
        let _ = app.emit("social-download-progress", last_progress.clone());
        return;
    }

    // Check for merger (means merging video+audio — almost done)
    if merger_re.is_match(line) {
        last_progress.progress = 0.95;
        last_progress.speed = None;
        last_progress.eta = None;
        last_progress.status = "merging".to_string();
        last_progress.download_id = download_id.to_string();
        let _ = app.emit("social-download-progress", last_progress.clone());
        return;
    }

    // Check for progress line
    if let Some(caps) = progress_re.captures(line) {
        let raw_percent: f64 = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0.0);

        *last_raw_percent = raw_percent;

        let size_value: f64 = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0.0);

        let size_unit = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        let size_str = format!("{}{}B", size_value, size_unit);
        let total_bytes = parse_size_to_bytes(&size_str);

        let downloaded_bytes = if total_bytes > 0 {
            (total_bytes as f64 * raw_percent / 100.0) as i64
        } else {
            0
        };

        let speed = caps.get(4).map(|m| m.as_str().to_string());
        let eta = caps.get(5).map(|m| m.as_str().to_string());

        // Scale progress for multi-stream: pass 0 = 0-45%, pass 1 = 45-90%, merge = 90-100%
        let overall_progress = if is_multi_stream {
            let pass_frac = raw_percent / 100.0;
            match *download_pass {
                0 => pass_frac * 0.45,
                1 => 0.45 + pass_frac * 0.45,
                _ => 0.9 + pass_frac * 0.1,
            }
        } else {
            raw_percent / 100.0
        };

        last_progress.progress = overall_progress.min(0.99);
        last_progress.downloaded_bytes = downloaded_bytes;
        last_progress.total_bytes = if total_bytes > 0 {
            Some(total_bytes)
        } else {
            None
        };
        last_progress.speed = speed;
        last_progress.eta = eta;
        last_progress.status = "downloading".to_string();
        last_progress.download_id = download_id.to_string();
        last_progress.filename = current_filename.clone();

        // Update DB progress
        {
            let db_state = app.state::<Database>();
            let conn = db_state.conn.lock();
            if let Ok(c) = conn {
                c.execute(
                    "UPDATE social_downloads SET progress = ?2, downloaded_bytes = ?3, total_bytes = ?4 WHERE id = ?1",
                    params![download_id, overall_progress, downloaded_bytes, last_progress.total_bytes],
                ).ok();
            }
        }

        let _ = app.emit("social-download-progress", last_progress.clone());
    }
}

#[tauri::command]
pub async fn cancel_social_download(
    download_id: String,
    dm: tauri::State<'_, SocialDownloadManager>,
    db: State<'_, Database>,
) -> Result<(), String> {
    if let Some(tx) = dm.active.lock().await.remove(&download_id) {
        tx.send(true).ok();
    }
    if let Ok(conn) = db.conn.lock() {
        conn.execute(
            "UPDATE social_downloads SET status = 'cancelled' WHERE id = ?1",
            params![download_id],
        ).ok();
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SocialDownloadRecord {
    pub id: String,
    pub url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub platform: String,
    pub format_label: Option<String>,
    pub output_dir: Option<String>,
    pub file_path: Option<String>,
    pub status: String,
    pub progress: f64,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[tauri::command]
pub async fn get_social_downloads(db: State<'_, Database>) -> Result<Vec<SocialDownloadRecord>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, url, title, thumbnail, platform, format_label, output_dir, file_path,
                    status, progress, downloaded_bytes, total_bytes, created_at, completed_at
             FROM social_downloads ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(SocialDownloadRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                thumbnail: row.get(3)?,
                platform: row.get(4)?,
                format_label: row.get(5)?,
                output_dir: row.get(6)?,
                file_path: row.get(7)?,
                status: row.get(8)?,
                progress: row.get(9)?,
                downloaded_bytes: row.get(10)?,
                total_bytes: row.get(11)?,
                created_at: row.get(12)?,
                completed_at: row.get(13)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| e.to_string())?);
    }
    Ok(results)
}

#[tauri::command]
pub async fn clear_social_downloads(db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM social_downloads WHERE status IN ('completed', 'failed', 'cancelled')", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}
