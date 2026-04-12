use crate::db::models::Channel;
use crate::db::Database;
use rusqlite::params;
use tauri::State;

#[derive(Debug)]
struct GroupScore {
    group_name: String,
    score: f64,
}

#[tauri::command]
pub async fn get_recommendations(
    limit: Option<i64>,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let limit = limit.unwrap_or(20);
    let conn = db.conn.lock().unwrap();

    // Check if there is any viewing history
    let history_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM viewing_history", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    if history_count == 0 {
        // No history: return random channels
        let mut stmt = conn
            .prepare(
                "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
                 FROM channels ORDER BY RANDOM() LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(Channel {
                    id: row.get(0)?,
                    playlist_id: row.get(1)?,
                    name: row.get(2)?,
                    group_name: row.get(3)?,
                    stream_url: row.get(4)?,
                    logo_url: row.get(5)?,
                    epg_id: row.get(6)?,
                    content_type: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut channels = Vec::new();
        for row in rows {
            channels.push(row.map_err(|e| e.to_string())?);
        }
        return Ok(channels);
    }

    // Get viewing stats per group
    let mut stmt = conn
        .prepare(
            "SELECT group_name,
                    COUNT(*) as view_count,
                    SUM(duration_seconds) as total_seconds,
                    CAST((julianday('now') - julianday(MAX(started_at))) AS REAL) as days_since_last
             FROM viewing_history
             GROUP BY group_name",
        )
        .map_err(|e| e.to_string())?;

    let group_stats: Vec<(String, i64, i64, f64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    if group_stats.is_empty() {
        return Ok(Vec::new());
    }

    // Find max values for normalization
    let max_count = group_stats.iter().map(|g| g.1).max().unwrap_or(1) as f64;
    let max_seconds = group_stats.iter().map(|g| g.2).max().unwrap_or(1) as f64;

    // Calculate scores
    let mut group_scores: Vec<GroupScore> = group_stats
        .iter()
        .map(|(group_name, count, total_secs, days_since)| {
            let frequency_score = (*count as f64) / max_count;
            let recency_score = (-days_since / 7.0).exp(); // exponential decay over 7 days
            let affinity_score = (*total_secs as f64) / max_seconds;

            let score = frequency_score * 0.4 + recency_score * 0.3 + affinity_score * 0.3;

            GroupScore {
                group_name: group_name.clone(),
                score,
            }
        })
        .collect();

    // Sort by score descending
    group_scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Get unwatched channels from top-scored groups
    let mut results = Vec::new();
    let watched_ids: Vec<i64> = conn
        .prepare("SELECT DISTINCT channel_id FROM viewing_history")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    for gs in &group_scores {
        if results.len() >= limit as usize {
            break;
        }

        let remaining = limit - results.len() as i64;
        let mut stmt = conn
            .prepare(
                "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
                 FROM channels
                 WHERE group_name = ?1
                 ORDER BY RANDOM()
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![gs.group_name, remaining], |row| {
                Ok(Channel {
                    id: row.get(0)?,
                    playlist_id: row.get(1)?,
                    name: row.get(2)?,
                    group_name: row.get(3)?,
                    stream_url: row.get(4)?,
                    logo_url: row.get(5)?,
                    epg_id: row.get(6)?,
                    content_type: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            if let Ok(channel) = row {
                // Prefer unwatched but include watched if needed
                if !watched_ids.contains(&channel.id) {
                    results.push(channel);
                }
            }
        }
    }

    // If we still don't have enough, fill with random unwatched channels
    if results.len() < limit as usize {
        let still_needed = limit - results.len() as i64;
        let existing_ids: Vec<i64> = results.iter().map(|c| c.id).collect();

        let mut stmt = conn
            .prepare(
                "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
                 FROM channels
                 ORDER BY RANDOM()
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![still_needed + existing_ids.len() as i64], |row| {
                Ok(Channel {
                    id: row.get(0)?,
                    playlist_id: row.get(1)?,
                    name: row.get(2)?,
                    group_name: row.get(3)?,
                    stream_url: row.get(4)?,
                    logo_url: row.get(5)?,
                    epg_id: row.get(6)?,
                    content_type: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            if results.len() >= limit as usize {
                break;
            }
            if let Ok(channel) = row {
                if !existing_ids.contains(&channel.id) {
                    results.push(channel);
                }
            }
        }
    }

    Ok(results)
}
