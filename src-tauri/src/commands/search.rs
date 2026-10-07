//! Global search across every playlist: live channels, movies and series.
//!
//! Names are normalized once (Arabic letter variants, diacritics, case) into an
//! in-memory index, so each keystroke scores ~70k entries in memory instead of
//! running LIKE scans. The index rebuilds itself when the library changes.

use crate::commands::channels::normalize_text;
use crate::db::models::{Channel, MediaChannel};
use crate::db::Database;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;
use tauri::State;

struct Entry {
    channel: Channel,
    rating: Option<f64>,
    year: Option<i64>,
    genre: Option<String>,
    norm_name: String,
    words: Vec<String>,
    norm_group: String,
}

struct Index {
    /// Changes whenever channels are added, removed or refreshed
    stamp: String,
    entries: Vec<Entry>,
}

#[derive(Default)]
pub struct SearchIndex(Mutex<Option<Index>>);

#[derive(Serialize, Default)]
pub struct SearchCounts {
    pub live: usize,
    pub vod: usize,
    pub series: usize,
}

#[derive(Serialize)]
pub struct GlobalSearchResult {
    pub live: Vec<MediaChannel>,
    pub vod: Vec<MediaChannel>,
    pub series: Vec<MediaChannel>,
    /// All matches per type (the lists above are the best few)
    pub counts: SearchCounts,
    pub elapsed_ms: f64,
}

fn library_stamp(db: &Database) -> Result<String, String> {
    let conn = db.conn.lock().unwrap();
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM channels) || ':' || (SELECT COALESCE(MAX(id), 0) FROM channels)
                || ':' || (SELECT COALESCE(GROUP_CONCAT(last_updated_at), '') FROM playlists)",
        [],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

fn build(db: &Database, stamp: String) -> Result<Index, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, playlist_id, name, group_name, stream_url, logo_url, epg_id, content_type, created_at,
                    rating, year, genre
             FROM channels",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                Channel {
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
                row.get::<_, Option<f64>>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<String>>(11)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for row in rows {
        let (channel, rating, year, genre) = row.map_err(|e| e.to_string())?;
        // Punctuation splits words: "Spider-Man:" → "spider", "man"
        let norm_name = normalize_text(&channel.name)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>();
        let words = norm_name.split_whitespace().map(String::from).collect();
        let norm_group = normalize_text(&channel.group_name);
        entries.push(Entry { channel, rating, year, genre, norm_name, words, norm_group });
    }
    Ok(Index { stamp, entries })
}

/// Relevance of one entry for the query words (0 = no match).
fn score(e: &Entry, query: &str, words: &[String]) -> i32 {
    if e.norm_name.trim() == query {
        return 200;
    }
    let mut score = 0;
    let mut matched = 0;
    for qw in words {
        let w = qw.as_str();
        if e.words.iter().any(|nw| nw == w) {
            score += 20;
        } else if e.words.iter().any(|nw| nw.starts_with(w)) {
            score += 15;
        } else if w.chars().count() >= 3 && e.norm_name.contains(w) {
            score += 8;
        } else if e.norm_group.contains(w) {
            score += 2;
        } else {
            continue;
        }
        matched += 1;
    }
    // Every word must match something; a stray one means it's not this title
    if matched < words.len() {
        return 0;
    }
    score += 20;
    if e.norm_name.trim_start().starts_with(query) {
        score += 25;
    }
    // Shorter names are closer matches ("bein sports 1" over "bein sports 1 hd backup")
    score -= (e.words.len() as i32 - words.len() as i32).clamp(0, 10);
    score
}

/// Warms the index (e.g. when the search box opens) so the first query is instant.
#[tauri::command]
pub async fn search_warmup(db: State<'_, Database>, index: State<'_, SearchIndex>) -> Result<(), String> {
    ensure(&db, &index)
}

fn ensure(db: &Database, index: &SearchIndex) -> Result<(), String> {
    let stamp = library_stamp(db)?;
    let mut guard = index.0.lock().unwrap();
    if guard.as_ref().map_or(true, |i| i.stamp != stamp) {
        *guard = Some(build(db, stamp)?);
    }
    Ok(())
}

/// Best matches per type across all playlists.
#[tauri::command]
pub async fn global_search(
    query: String,
    per_type: Option<usize>,
    db: State<'_, Database>,
    index: State<'_, SearchIndex>,
) -> Result<GlobalSearchResult, String> {
    let started = Instant::now();
    let per_type = per_type.unwrap_or(12).clamp(1, 100);
    let norm_query = normalize_text(&query)
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>();
    let norm_query = norm_query.split_whitespace().collect::<Vec<_>>().join(" ");
    let words: Vec<String> = norm_query.split(' ').filter(|w| !w.is_empty()).map(String::from).collect();

    let mut result = GlobalSearchResult {
        live: vec![],
        vod: vec![],
        series: vec![],
        counts: SearchCounts::default(),
        elapsed_ms: 0.0,
    };
    if words.is_empty() {
        return Ok(result);
    }

    ensure(&db, &index)?;
    let guard = index.0.lock().unwrap();
    let entries = &guard.as_ref().ok_or("Search index unavailable")?.entries;

    let mut hits: [Vec<(i32, &Entry)>; 3] = [vec![], vec![], vec![]];
    for e in entries {
        let s = score(e, &norm_query, &words);
        if s <= 0 {
            continue;
        }
        let bucket = match e.channel.content_type.as_str() {
            "vod" => 1,
            "series" => 2,
            _ => 0,
        };
        hits[bucket].push((s, e));
    }

    result.counts = SearchCounts { live: hits[0].len(), vod: hits[1].len(), series: hits[2].len() };
    let pick = |list: &mut Vec<(i32, &Entry)>| -> Vec<MediaChannel> {
        // Relevance first; then rating and recency break ties
        list.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.rating.unwrap_or(0.0).partial_cmp(&a.1.rating.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| b.1.year.unwrap_or(0).cmp(&a.1.year.unwrap_or(0)))
        });
        list.iter()
            .take(per_type)
            .map(|(_, e)| MediaChannel {
                channel: e.channel.clone(),
                rating: e.rating,
                year: e.year,
                genre: e.genre.clone(),
            })
            .collect()
    };
    result.live = pick(&mut hits[0]);
    result.vod = pick(&mut hits[1]);
    result.series = pick(&mut hits[2]);
    result.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(result)
}

#[cfg(test)]
mod bench {
    use super::*;

    /// Manual benchmark against a copy of a real library:
    /// JOTV_BENCH_DIR=/path/with/jotv.db cargo test --release bench_global_search -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_global_search() {
        let dir = std::env::var("JOTV_BENCH_DIR").expect("JOTV_BENCH_DIR");
        let db = Database::new(dir.into()).unwrap();
        let index = SearchIndex::default();
        let t = Instant::now();
        ensure(&db, &index).unwrap();
        println!("build: {:?}", t.elapsed());
        for q in ["bein", "bein sports 1", "spider", "نصف الام", "الاهلي", "mbc", "x"] {
            let t = Instant::now();
            ensure(&db, &index).unwrap();
            let guard = index.0.lock().unwrap();
            let entries = &guard.as_ref().unwrap().entries;
            let nq = normalize_text(q);
            let words: Vec<String> = nq.split_whitespace().map(String::from).collect();
            let mut hits: Vec<(i32, &Entry)> = entries.iter().map(|e| (score(e, &nq, &words), e)).filter(|h| h.0 > 0).collect();
            hits.sort_by(|a, b| b.0.cmp(&a.0));
            let top: Vec<_> = hits.iter().take(3).map(|h| h.1.channel.name.clone()).collect();
            println!("{:>14} → {:>5} hits in {:?}  top: {:?}", q, hits.len(), t.elapsed(), top);
        }
    }
}
