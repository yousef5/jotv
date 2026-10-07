//! Video library ("reels"): finished social downloads plus imported local files,
//! organized into categories/sub-categories with tags. Thumbnails and durations
//! come from ffmpeg/ffprobe on the local file, so they never expire like CDN links.

use crate::db::Database;
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Serialize)]
pub struct Reel {
    pub id: String,
    /// Display name (user's name, else the original title)
    pub title: String,
    pub original_title: String,
    pub url: String,
    pub platform: String,
    pub file_path: String,
    /// Remote thumbnail from the source site (may expire)
    pub thumbnail: Option<String>,
    /// Local thumbnail generated from the file
    pub thumb_path: Option<String>,
    pub category_id: Option<i64>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub duration: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: Option<i64>,
    pub plays: i64,
    pub last_played_at: Option<String>,
    pub created_at: String,
    /// "video" or "image"
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReelCategory {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub color: Option<String>,
    pub sort: i64,
}

const REEL_COLUMNS: &str = "id, COALESCE(NULLIF(name, ''), title), title, url, platform, file_path, thumbnail, thumb_path,
     category_id, tags, favorite, duration, width, height, file_size, plays, last_played_at, created_at, media_type";

fn row_to_reel(row: &rusqlite::Row) -> rusqlite::Result<Reel> {
    let tags: String = row.get(9)?;
    Ok(Reel {
        id: row.get(0)?,
        title: row.get(1)?,
        original_title: row.get(2)?,
        url: row.get(3)?,
        platform: row.get(4)?,
        file_path: row.get(5)?,
        thumbnail: row.get(6)?,
        thumb_path: row.get(7)?,
        category_id: row.get(8)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        favorite: row.get::<_, i64>(10)? != 0,
        duration: row.get(11)?,
        width: row.get(12)?,
        height: row.get(13)?,
        file_size: row.get(14)?,
        plays: row.get(15)?,
        last_played_at: row.get(16)?,
        created_at: row.get(17)?,
        kind: row.get(18)?,
    })
}

fn get_reel(conn: &Connection, id: &str) -> Result<Reel, String> {
    conn.query_row(
        &format!("SELECT {} FROM social_downloads WHERE id = ?1", REEL_COLUMNS),
        params![id],
        row_to_reel,
    )
    .map_err(|e| format!("Video not found: {}", e))
}

/// Trimmed, de-duplicated (case-insensitive) tags without a leading '#'.
pub(crate) fn clean_tags<I: IntoIterator<Item = String>>(tags: I) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t = t.trim().trim_start_matches('#').trim().to_string();
        if t.is_empty() || t.chars().count() > 40 {
            continue;
        }
        if !out.iter().any(|o| o.to_lowercase() == t.to_lowercase()) {
            out.push(t);
        }
    }
    out
}

/// "#hashtags" in a title become tags.
pub(crate) fn hashtags(title: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"#([\p{L}\p{N}_]{2,40})").unwrap());
    clean_tags(re.captures_iter(title).map(|c| c[1].replace('_', " ")))
}

/// Imported photos are copied here, so moving or deleting the original never breaks them.
fn images_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("library-images");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn now_millis() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn thumbs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("reel-thumbs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

// ── Library ─────────────────────────────────────────────────────────────────

/// Older downloads saved the path of a piece yt-dlp deletes after merging
/// ("Name.f251.webm" → the real file is "Name.mp4"). Finds the merged file.
fn find_merged(path: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(.*)\.f[0-9A-Za-z_-]+\.[0-9A-Za-z]+$").unwrap());
    let p = Path::new(path);
    let name = p.file_name()?.to_str()?;
    let stem = re.captures(name)?.get(1)?.as_str().to_string();
    let dir = p.parent()?;
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|c| {
            c.file_stem().and_then(|s| s.to_str()) == Some(stem.as_str())
                && (VIDEO_EXTS.contains(&ext_of(c).as_str()) || ["mp3", "m4a", "opus"].contains(&ext_of(c).as_str()))
                && c.is_file()
        })
        .collect();
    // Prefer video over audio
    found.sort_by_key(|c| !VIDEO_EXTS.contains(&ext_of(c).as_str()));
    found.first().map(|c| c.to_string_lossy().into_owned())
}

/// Every finished video whose file is still on disk.
#[tauri::command]
pub async fn reels_list(db: State<'_, Database>) -> Result<Vec<Reel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM social_downloads
             WHERE status = 'completed' AND file_path IS NOT NULL
             ORDER BY created_at DESC",
            REEL_COLUMNS
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_to_reel).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut repaired = Vec::new();
    for r in rows {
        let mut reel = r.map_err(|e| e.to_string())?;
        if !Path::new(&reel.file_path).exists() {
            match find_merged(&reel.file_path) {
                Some(real) => {
                    repaired.push((reel.id.clone(), real.clone()));
                    reel.file_path = real;
                }
                None => continue,
            }
        }
        out.push(reel);
    }
    drop(stmt);
    for (id, path) in repaired {
        conn.execute("UPDATE social_downloads SET file_path = ?2 WHERE id = ?1", params![id, path]).ok();
    }
    Ok(out)
}

#[tauri::command]
pub async fn reel_update(
    id: String,
    name: Option<String>,
    category_id: Option<i64>,
    tags: Vec<String>,
    favorite: bool,
    db: State<'_, Database>,
) -> Result<Reel, String> {
    let conn = db.conn.lock().unwrap();
    let tags = serde_json::to_string(&clean_tags(tags)).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE social_downloads SET name = ?2, category_id = ?3, tags = ?4, favorite = ?5 WHERE id = ?1",
        params![id, name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()), category_id, tags, favorite as i64],
    )
    .map_err(|e| e.to_string())?;
    get_reel(&conn, &id)
}

/// Move several videos into a category (None = uncategorized).
#[tauri::command]
pub async fn reels_move(ids: Vec<String>, category_id: Option<i64>, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    for id in ids {
        conn.execute("UPDATE social_downloads SET category_id = ?2 WHERE id = ?1", params![id, category_id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Add tags to several videos.
#[tauri::command]
pub async fn reels_tag(ids: Vec<String>, tags: Vec<String>, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    for id in ids {
        let reel = get_reel(&conn, &id)?;
        let merged = clean_tags(reel.tags.into_iter().chain(tags.iter().cloned()));
        let json = serde_json::to_string(&merged).map_err(|e| e.to_string())?;
        conn.execute("UPDATE social_downloads SET tags = ?2 WHERE id = ?1", params![id, json])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Remove videos from the library; optionally delete the files too.
#[tauri::command]
pub async fn reels_delete(app: AppHandle, ids: Vec<String>, delete_files: bool, db: State<'_, Database>) -> Result<(), String> {
    let own = images_dir(&app)?;
    let thumbs = thumbs_dir(&app)?;
    let conn = db.conn.lock().unwrap();
    for id in ids {
        if let Ok(reel) = get_reel(&conn, &id) {
            if let Some(thumb) = reel.thumb_path.as_deref().filter(|t| Path::new(t).starts_with(&thumbs)) {
                std::fs::remove_file(thumb).ok();
            }
            // A photo's library copy belongs to the app
            if delete_files || Path::new(&reel.file_path).starts_with(&own) {
                std::fs::remove_file(&reel.file_path).ok();
            }
        }
        conn.execute("DELETE FROM social_downloads WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "m4v", "ts", "flv", "3gp"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"];

fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

fn is_media(p: &Path) -> bool {
    let ext = ext_of(p);
    p.is_file() && (VIDEO_EXTS.contains(&ext.as_str()) || IMAGE_EXTS.contains(&ext.as_str()))
}

/// Videos and photos in `p` (a file, or a folder searched a few levels deep).
fn collect_media(p: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(p) else { return };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for child in paths {
            if !child.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')) {
                collect_media(&child, depth - 1, out);
            }
        }
    } else if is_media(p) {
        out.push(p.to_path_buf());
    }
}

/// Adds local videos and photos (or everything in dropped folders) to the
/// library. Videos are referenced in place; photos are copied into the app's
/// data folder. Returns how many were added; files already added are skipped.
#[tauri::command]
pub async fn reels_import(
    app: AppHandle,
    paths: Vec<String>,
    category_id: Option<i64>,
    db: State<'_, Database>,
) -> Result<usize, String> {
    let mut files = Vec::new();
    for path in &paths {
        collect_media(Path::new(path), 4, &mut files);
    }
    let own = images_dir(&app)?;
    let mut added = 0;
    for (i, p) in files.iter().enumerate() {
        let source = p.to_string_lossy().into_owned();
        let image = IMAGE_EXTS.contains(&ext_of(p).as_str());
        {
            // Photos remember where they came from in `url`, videos by path
            let conn = db.conn.lock().unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM social_downloads WHERE file_path = ?1 OR (media_type = 'image' AND url = ?2)",
                    params![source, format!("file://{}", source)],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if exists {
                continue;
            }
        }
        let id = format!("local-{}-{}", now_millis(), i);
        let file_path = if image {
            let target = own.join(format!("{}.{}", id, ext_of(p)));
            std::fs::copy(p, &target).map_err(|e| format!("Couldn't copy {}: {}", source, e))?;
            target.to_string_lossy().into_owned()
        } else {
            source.clone()
        };
        let title = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
        let tags = serde_json::to_string(&hashtags(&title)).unwrap_or_else(|_| "[]".into());
        let size = std::fs::metadata(p).map(|m| m.len() as i64).ok();
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO social_downloads (id, url, title, platform, file_path, status, progress, completed_at, category_id, tags, file_size, in_history, media_type)
             VALUES (?1, ?2, ?3, 'Local', ?4, 'completed', 1.0, datetime('now'), ?5, ?6, ?7, 0, ?8)",
            params![id, format!("file://{}", source), title, file_path, category_id, tags, size, if image { "image" } else { "video" }],
        )
        .map_err(|e| e.to_string())?;
        added += 1;
    }
    Ok(added)
}

/// Uses an image as a video's cover (`None` puts back a frame from the video).
#[tauri::command]
pub async fn reel_set_cover(app: AppHandle, id: String, image: Option<String>) -> Result<Reel, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Database>();
        let thumbs = thumbs_dir(&app)?;
        let old = {
            let conn = db.conn.lock().unwrap();
            get_reel(&conn, &id)?.thumb_path
        };
        let new_thumb = match image {
            Some(src) => {
                let target = thumbs.join(format!("{}-cover-{}.jpg", id.replace(['/', '\\', '.'], "_"), now_millis()));
                let ok = Command::new("ffmpeg")
                    .args(["-y", "-v", "error", "-i", &src, "-frames:v", "1", "-vf", "scale='min(720,iw)':-2", "-q:v", "3"])
                    .arg(&target)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ok || !target.exists() {
                    return Err("That image couldn't be read".to_string());
                }
                Some(target.to_string_lossy().into_owned())
            }
            None => None,
        };
        if let Some(old) = old.filter(|o| Path::new(o).starts_with(&thumbs)) {
            std::fs::remove_file(old).ok();
        }
        {
            let conn = db.conn.lock().unwrap();
            conn.execute("UPDATE social_downloads SET thumb_path = ?2 WHERE id = ?1", params![id, new_thumb])
                .map_err(|e| e.to_string())?;
        }
        match new_thumb {
            Some(_) => {
                let conn = db.conn.lock().unwrap();
                get_reel(&conn, &id)
            }
            None => probe_and_store(&app, &id),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Fills in duration/size from ffprobe and makes a local thumbnail with ffmpeg.
/// Blocking: ffmpeg on a big file can take a moment.
pub(crate) fn probe_and_store(app: &AppHandle, id: &str) -> Result<Reel, String> {
    let db = app.state::<Database>();
    let (file, image) = {
        let conn = db.conn.lock().unwrap();
        let r = get_reel(&conn, id)?;
        (r.file_path, r.kind == "image")
    };
    let thumb_target = thumbs_dir(app)?.join(format!("{}.jpg", id.replace(['/', '\\', '.'], "_")));

    let meta = Command::new("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", "-select_streams", "v:0", &file])
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());
    let num = |ptr: &str| -> Option<serde_json::Value> { meta.as_ref().and_then(|m| m.pointer(ptr)).cloned() };
    let duration = num("/format/duration")
        .and_then(|d| d.as_str().and_then(|d| d.parse::<f64>().ok()))
        .filter(|_| !image);
    let size = num("/format/size").and_then(|d| d.as_str().and_then(|d| d.parse::<i64>().ok()));
    let width = num("/streams/0/width").and_then(|w| w.as_i64());
    let height = num("/streams/0/height").and_then(|h| h.as_i64());

    // A frame a little way in (first frames are often black)
    // (photos: just a smaller copy)
    let at = duration.map(|d| (d * 0.15).clamp(0.0, 8.0)).unwrap_or(if image { 0.0 } else { 1.0 });
    let ok = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-ss", &format!("{:.2}", at), "-i", &file, "-frames:v", "1", "-vf", "scale='min(480,iw)':-2", "-q:v", "4"])
        .arg(&thumb_target)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let thumb = (ok && thumb_target.exists()).then(|| thumb_target.to_string_lossy().into_owned());

    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE social_downloads SET duration = COALESCE(?2, duration), file_size = COALESCE(?3, file_size),
                width = COALESCE(?4, width), height = COALESCE(?5, height), thumb_path = COALESCE(?6, thumb_path)
         WHERE id = ?1",
        params![id, duration, size, width, height, thumb],
    )
    .map_err(|e| e.to_string())?;
    get_reel(&conn, id)
}

#[tauri::command]
pub async fn reel_probe(app: AppHandle, id: String) -> Result<Reel, String> {
    tauri::async_runtime::spawn_blocking(move || probe_and_store(&app, &id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reel_played(id: String, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE social_downloads SET plays = plays + 1, last_played_at = datetime('now') WHERE id = ?1",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Categories ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn reel_categories(db: State<'_, Database>) -> Result<Vec<ReelCategory>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, parent_id, color, sort FROM reel_categories ORDER BY sort, name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ReelCategory { id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, color: r.get(3)?, sort: r.get(4)? })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Creates a category (no `id`) or renames/moves/recolors one. Only two levels:
/// a sub-category's parent must be a main category.
#[tauri::command]
pub async fn reel_category_save(
    id: Option<i64>,
    name: String,
    parent_id: Option<i64>,
    color: Option<String>,
    db: State<'_, Database>,
) -> Result<ReelCategory, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Category name can't be empty".into());
    }
    let conn = db.conn.lock().unwrap();
    if let Some(pid) = parent_id {
        let grand: Option<i64> = conn
            .query_row("SELECT parent_id FROM reel_categories WHERE id = ?1", params![pid], |r| r.get(0))
            .map_err(|_| "Parent category not found".to_string())?;
        if grand.is_some() || Some(pid) == id {
            return Err("Sub-categories can only go under a main category".into());
        }
    }
    let id = match id {
        Some(id) => {
            conn.execute(
                "UPDATE reel_categories SET name = ?2, parent_id = ?3, color = ?4 WHERE id = ?1",
                params![id, name, parent_id, color],
            )
            .map_err(|e| e.to_string())?;
            id
        }
        None => {
            let sort: i64 = conn
                .query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM reel_categories", [], |r| r.get(0))
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO reel_categories (name, parent_id, color, sort) VALUES (?1, ?2, ?3, ?4)",
                params![name, parent_id, color, sort],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };
    conn.query_row("SELECT id, name, parent_id, color, sort FROM reel_categories WHERE id = ?1", params![id], |r| {
        Ok(ReelCategory { id: r.get(0)?, name: r.get(1)?, parent_id: r.get(2)?, color: r.get(3)?, sort: r.get(4)? })
    })
    .map_err(|e| e.to_string())
}

/// Deletes a category and its sub-categories; their videos become uncategorized.
#[tauri::command]
pub async fn reel_category_delete(id: i64, db: State<'_, Database>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE social_downloads SET category_id = NULL
         WHERE category_id = ?1 OR category_id IN (SELECT id FROM reel_categories WHERE parent_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM reel_categories WHERE id = ?1 OR parent_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashtags() {
        assert_eq!(hashtags("مشاهد الزلزال #العربية #مصر #مصر"), vec!["العربية", "مصر"]);
        assert_eq!(hashtags("Goal of the year #football #Real_Madrid"), vec!["football", "Real Madrid"]);
        assert!(hashtags("No tags here #").is_empty());
    }

    #[test]
    fn test_find_merged() {
        let dir = std::env::temp_dir().join(format!("jotv-merge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Super Easy Paper Star.mp4"), b"x").unwrap();
        let piece = dir.join("Super Easy Paper Star.f2138838474180394a.m4a");
        assert_eq!(
            find_merged(piece.to_str().unwrap()).as_deref(),
            Some(dir.join("Super Easy Paper Star.mp4").to_str().unwrap())
        );
        assert_eq!(find_merged(dir.join("Other.f251.webm").to_str().unwrap()), None);
        assert_eq!(find_merged(dir.join("Plain.mp4").to_str().unwrap()), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_clean_tags() {
        let t = clean_tags(vec![" #Funny ".into(), "funny".into(), "".into(), "Cats".into()]);
        assert_eq!(t, vec!["Funny", "Cats"]);
    }
}
