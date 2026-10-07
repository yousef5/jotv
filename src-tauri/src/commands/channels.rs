use crate::db::models::{Channel, ChannelGroup, ContentTypeCount, DashboardStats, FacetCount, MediaChannel, MediaFacets};
use crate::db::Database;
use rusqlite::params;
use tauri::State;

/// Normalize Arabic text for fuzzy matching:
/// - Remove diacritics (tashkeel)
/// - Normalize alef variants (أ إ آ ٱ → ا)
/// - Normalize taa marbuta (ة → ه)
/// - Normalize yaa (ى → ي, ئ → ي)
/// - Normalize waw (ؤ → و)
/// - Remove tatweel (ـ)
/// - Lowercase Latin chars
pub(crate) fn normalize_text(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(*c as u32, 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED))
        .map(|c| match c {
            'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
            'ة' => 'ه',
            'ى' => 'ي',
            'ئ' => 'ي',
            'ؤ' => 'و',
            'ـ' => ' ',
            c => c,
        })
        .collect::<String>()
        .to_lowercase()
}

/// Generate fuzzy SQL patterns from a query.
/// For each word, creates a pattern using the first 2+ chars for prefix matching.
/// "سي عمر" → ["%سي%", "%عم%"] so it matches "سي عميرر"
fn fuzzy_patterns(query: &str) -> Vec<String> {
    let normalized = normalize_text(query);
    normalized
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|word| {
            let chars: Vec<char> = word.chars().collect();
            if chars.len() <= 2 {
                format!("%{}%", word)
            } else {
                // Use first 2 chars as prefix pattern for fuzzy match
                let prefix: String = chars[..2].iter().collect();
                format!("%{}%", prefix)
            }
        })
        .collect()
}

/// Score how well a channel name matches a query (higher = better).
fn match_score(name: &str, group: &str, query: &str) -> i32 {
    let norm_name = normalize_text(name);
    let norm_group = normalize_text(group);
    let norm_query = normalize_text(query);

    // Exact match
    if norm_name == norm_query { return 100; }

    let query_words: Vec<&str> = norm_query.split_whitespace().collect();
    let name_words: Vec<&str> = norm_name.split_whitespace().collect();

    let mut score: i32 = 0;
    let mut matched_words = 0;

    for qw in &query_words {
        // Exact word match
        if name_words.iter().any(|nw| nw == qw) {
            score += 20;
            matched_words += 1;
            continue;
        }
        // Name word starts with query word
        if name_words.iter().any(|nw| nw.starts_with(qw)) {
            score += 15;
            matched_words += 1;
            continue;
        }
        // Query word starts with name word (user typed more)
        if name_words.iter().any(|nw| qw.starts_with(nw)) {
            score += 12;
            matched_words += 1;
            continue;
        }
        // Name contains query word as substring
        if norm_name.contains(qw) {
            score += 10;
            matched_words += 1;
            continue;
        }
        // Prefix match (first 2 chars of any word)
        let prefix: String = qw.chars().take(2).collect();
        if !prefix.is_empty() && name_words.iter().any(|nw| nw.starts_with(&prefix)) {
            score += 5;
            matched_words += 1;
            continue;
        }
        // Check group name
        if norm_group.contains(qw) {
            score += 3;
            matched_words += 1;
        }
    }

    // Bonus: all query words matched
    if matched_words == query_words.len() {
        score += 20;
    }

    // Name starts with query
    if norm_name.starts_with(&norm_query) {
        score += 10;
    }

    score
}

#[tauri::command]
pub async fn get_channels(
    playlist_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
             FROM channels WHERE playlist_id = ?1 ORDER BY group_name, name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id], |row| {
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
    Ok(channels)
}

#[tauri::command]
pub async fn get_channels_by_type(
    playlist_id: i64,
    content_type: String,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
             FROM channels WHERE playlist_id = ?1 AND content_type = ?2 ORDER BY group_name, name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id, content_type], |row| {
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
    Ok(channels)
}

#[tauri::command]
pub async fn get_content_type_counts(
    playlist_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<ContentTypeCount>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT content_type, COUNT(*) as count FROM channels
             WHERE playlist_id = ?1 GROUP BY content_type ORDER BY content_type",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id], |row| {
            Ok(ContentTypeCount {
                content_type: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut counts = Vec::new();
    for row in rows {
        counts.push(row.map_err(|e| e.to_string())?);
    }
    Ok(counts)
}

/// Get groups for a specific content type within a playlist.
#[tauri::command]
pub async fn get_groups_by_type(
    playlist_id: i64,
    content_type: String,
    db: State<'_, Database>,
) -> Result<Vec<ChannelGroup>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT group_name, COUNT(*) as count FROM channels
             WHERE playlist_id = ?1 AND content_type = ?2
             GROUP BY group_name ORDER BY group_name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id, content_type], |row| {
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

/// Get recently added channels for a playlist and content type (newest first).
#[tauri::command]
pub async fn get_recently_added(
    playlist_id: i64,
    content_type: String,
    limit: i64,
    offset: Option<i64>,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
             FROM channels
             WHERE playlist_id = ?1 AND content_type = ?2 AND added_on_server > 0
             ORDER BY added_on_server DESC, id DESC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id, content_type, limit, offset.unwrap_or(0)], |row| {
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
    Ok(channels)
}

/// Get channels for a specific group within a content type, with pagination.
#[tauri::command]
pub async fn get_channels_by_group(
    playlist_id: i64,
    content_type: String,
    group_name: String,
    limit: i64,
    offset: i64,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
             FROM channels
             WHERE playlist_id = ?1 AND content_type = ?2 AND group_name = ?3
             ORDER BY name
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![playlist_id, content_type, group_name, limit, offset], |row| {
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
    Ok(channels)
}

/// Fuzzy smart search within a playlist + content type.
/// Handles Arabic normalization, prefix matching, and relevance ranking.
#[tauri::command]
pub async fn search_channels_in_playlist(
    playlist_id: i64,
    content_type: String,
    query: String,
    limit: i64,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();

    // Generate fuzzy patterns from query
    let patterns = fuzzy_patterns(&query);
    let plain_pattern = format!("%{}%", normalize_text(&query));

    // Build WHERE clause: match any fuzzy pattern OR the plain normalized pattern
    let mut conditions: Vec<String> = vec![
        format!("LOWER(name) LIKE '{}'", plain_pattern.replace('\'', "''")),
        format!("LOWER(group_name) LIKE '{}'", plain_pattern.replace('\'', "''")),
    ];
    for p in &patterns {
        conditions.push(format!("LOWER(name) LIKE '{}'", p.replace('\'', "''")));
    }
    let where_clause = conditions.join(" OR ");

    let sql = format!(
        "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
         FROM channels
         WHERE playlist_id = ?1 AND content_type = ?2 AND ({})
         LIMIT 500",
        where_clause
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![playlist_id, content_type], |row| {
            Ok(Channel {
                id: row.get(0)?, playlist_id: row.get(1)?, name: row.get(2)?,
                group_name: row.get(3)?, stream_url: row.get(4)?, logo_url: row.get(5)?,
                epg_id: row.get(6)?, content_type: row.get(7)?, created_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut candidates: Vec<Channel> = Vec::new();
    for row in rows {
        candidates.push(row.map_err(|e| e.to_string())?);
    }

    // Score and sort by relevance
    let mut scored: Vec<(i32, Channel)> = candidates
        .into_iter()
        .map(|ch| {
            let score = match_score(&ch.name, &ch.group_name, &query);
            (score, ch)
        })
        .filter(|(score, _)| *score > 0)
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(limit as usize);

    Ok(scored.into_iter().map(|(_, ch)| ch).collect())
}

/// Global fuzzy search across ALL playlists and content types.
#[tauri::command]
pub async fn search_channels(
    query: String,
    db: State<'_, Database>,
) -> Result<Vec<Channel>, String> {
    let conn = db.conn.lock().unwrap();

    let patterns = fuzzy_patterns(&query);
    let plain_pattern = format!("%{}%", normalize_text(&query));

    let mut conditions: Vec<String> = vec![
        format!("LOWER(name) LIKE '{}'", plain_pattern.replace('\'', "''")),
        format!("LOWER(group_name) LIKE '{}'", plain_pattern.replace('\'', "''")),
    ];
    for p in &patterns {
        conditions.push(format!("LOWER(name) LIKE '{}'", p.replace('\'', "''")));
    }
    let where_clause = conditions.join(" OR ");

    let sql = format!(
        "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
         FROM channels WHERE {} LIMIT 500",
        where_clause
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Channel {
                id: row.get(0)?, playlist_id: row.get(1)?, name: row.get(2)?,
                group_name: row.get(3)?, stream_url: row.get(4)?, logo_url: row.get(5)?,
                epg_id: row.get(6)?, content_type: row.get(7)?, created_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut candidates: Vec<Channel> = Vec::new();
    for row in rows { candidates.push(row.map_err(|e| e.to_string())?); }

    let mut scored: Vec<(i32, Channel)> = candidates
        .into_iter()
        .map(|ch| (match_score(&ch.name, &ch.group_name, &query), ch))
        .filter(|(score, _)| *score > 0)
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(150);

    Ok(scored.into_iter().map(|(_, ch)| ch).collect())
}

#[tauri::command]
pub async fn get_dashboard_stats(db: State<'_, Database>) -> Result<DashboardStats, String> {
    let conn = db.conn.lock().unwrap();

    let total_channels: i64 = conn
        .query_row("SELECT COUNT(*) FROM channels", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_favorites: i64 = conn
        .query_row("SELECT COUNT(*) FROM favorites", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_playlists: i64 = conn
        .query_row("SELECT COUNT(*) FROM playlists", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_downloads: i64 = conn
        .query_row("SELECT COUNT(*) FROM downloads", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    Ok(DashboardStats {
        total_channels,
        total_favorites,
        total_playlists,
        total_downloads,
    })
}

/// Get a single channel by id.
#[tauri::command]
pub async fn get_channel(id: i64, db: State<'_, Database>) -> Result<Channel, String> {
    let conn = db.conn.lock().unwrap();
    conn.query_row(
        "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at
         FROM channels WHERE id = ?1",
        params![id],
        |row| {
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
        },
    )
    .map_err(|e| format!("Channel not found: {}", e))
}

/// Movies or series with sorting and filters, for the browse page.
/// `group` = None means the whole library of that type.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn browse_media(
    playlist_id: i64,
    content_type: String,
    group: Option<String>,
    sort: String,
    year: Option<i64>,
    genre: Option<String>,
    min_rating: Option<f64>,
    limit: i64,
    offset: i64,
    db: State<'_, Database>,
) -> Result<Vec<MediaChannel>, String> {
    let order = match sort.as_str() {
        "rating" => "rating IS NULL, rating DESC, added_on_server DESC",
        "year" => "year IS NULL, year DESC, added_on_server DESC",
        "name" => "name COLLATE NOCASE",
        _ => "added_on_server DESC, id DESC",
    };
    let sql = format!(
        "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at,
                rating, year, genre
         FROM channels
         WHERE playlist_id = ?1 AND content_type = ?2
           AND (?3 IS NULL OR group_name = ?3)
           AND (?4 IS NULL OR year = ?4)
           AND (?5 IS NULL OR genre LIKE '%' || ?5 || '%')
           AND (?6 IS NULL OR rating >= ?6)
         ORDER BY {order}
         LIMIT ?7 OFFSET ?8"
    );
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            params![playlist_id, content_type, group, year, genre, min_rating, limit, offset],
            |row| {
                Ok(MediaChannel {
                    channel: Channel {
                        id: row.get(0)?,
                        playlist_id: row.get(1)?,
                        name: row.get(2)?,
                        group_name: row.get(3)?,
                        stream_url: row.get(4)?,
                        logo_url: row.get(5)?,
                        epg_id: row.get(6)?,
                        content_type: row.get(7)?,
                        created_at: row.get(8)?,
                    },
                    rating: row.get(9)?,
                    year: row.get(10)?,
                    genre: row.get(11)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Years and genres present in a playlist's movies or series (optionally one group).
#[tauri::command]
pub async fn get_media_facets(
    playlist_id: i64,
    content_type: String,
    group: Option<String>,
    db: State<'_, Database>,
) -> Result<MediaFacets, String> {
    let conn = db.conn.lock().unwrap();

    let mut years = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT year, COUNT(*) FROM channels
                 WHERE playlist_id = ?1 AND content_type = ?2 AND (?3 IS NULL OR group_name = ?3) AND year IS NOT NULL
                 GROUP BY year ORDER BY year DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![playlist_id, content_type, group], |r| {
                Ok(FacetCount { value: r.get::<_, i64>(0)?, count: r.get(1)? })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            years.push(row.map_err(|e| e.to_string())?);
        }
    }

    // Genres come as "Comedy / Crime / Drama" or "Action, Drama"; count each one
    let mut genre_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT genre FROM channels
                 WHERE playlist_id = ?1 AND content_type = ?2 AND (?3 IS NULL OR group_name = ?3) AND genre IS NOT NULL",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![playlist_id, content_type, group], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let genre = row.map_err(|e| e.to_string())?;
            for part in genre.split(['/', ',', '|']) {
                let name = part.trim();
                if name.len() > 1 {
                    *genre_counts.entry(name.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    let mut genres: Vec<FacetCount<String>> = genre_counts
        .into_iter()
        .filter(|(_, n)| *n >= 3)
        .map(|(value, count)| FacetCount { value, count })
        .collect();
    genres.sort_by(|a, b| b.count.cmp(&a.count));

    let rated: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM channels
             WHERE playlist_id = ?1 AND content_type = ?2 AND (?3 IS NULL OR group_name = ?3) AND rating IS NOT NULL",
            params![playlist_id, content_type, group],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(MediaFacets { years, genres, rated })
}
