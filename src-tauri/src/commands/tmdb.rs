//! Optional artwork from TMDB (backdrops, title logos, cast photos, trailers).
//! Needs the user's own free TMDB key, stored in settings as `tmdb_api_key`.
//! Lookups are cached in the settings table, including "not found".

use crate::commands::channels::normalize_text;
use crate::db::Database;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

const IMG: &str = "https://image.tmdb.org/t/p";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbPerson {
    pub name: String,
    pub character: Option<String>,
    pub photo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbDetails {
    pub tmdb_id: i64,
    pub backdrop: Option<String>,
    /// Up to six text-free backdrops for a slideshow, best voted first (older cache entries have none)
    #[serde(default)]
    pub backdrops: Vec<String>,
    /// Transparent title treatment (PNG)
    pub logo: Option<String>,
    pub poster: Option<String>,
    pub overview: Option<String>,
    pub rating: Option<f64>,
    /// YouTube key
    pub trailer: Option<String>,
    pub cast: Vec<TmdbPerson>,
}

fn setting(db: &Database, key: &str) -> Option<String> {
    let conn = db.conn.lock().unwrap();
    conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).ok()
}

fn save_setting(db: &Database, key: &str, value: &str) {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )
    .ok();
}

/// v3 keys go in the query string; v4 read tokens (long JWTs) as a Bearer header.
fn request(client: &reqwest::Client, key: &str, url: &str, query: &[(&str, String)]) -> reqwest::RequestBuilder {
    let req = client.get(url).query(query);
    if key.len() > 60 {
        req.bearer_auth(key)
    } else {
        req.query(&[("api_key", key)])
    }
}

async fn get_json(client: &reqwest::Client, key: &str, url: &str, query: &[(&str, String)]) -> Result<Value, String> {
    let resp = request(client, key, url, query).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err("TMDB rejected the API key".into());
    }
    if !status.is_success() {
        return Err(format!("TMDB returned HTTP {}", status.as_u16()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())
}

fn img(path: Option<&str>, size: &str) -> Option<String> {
    path.filter(|p| p.starts_with('/')).map(|p| format!("{}/{}{}", IMG, size, p))
}

/// Best image from an `images.*` list: preferred languages first, then highest voted.
fn pick_image(list: Option<&Value>, langs: &[Option<&str>]) -> Option<String> {
    let items = list?.as_array()?;
    for lang in langs {
        let best = items
            .iter()
            .filter(|i| i.get("iso_639_1").and_then(|v| v.as_str()) == *lang)
            .max_by(|a, b| {
                let va = a.get("vote_average").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let vb = b.get("vote_average").and_then(|v| v.as_f64()).unwrap_or(0.0);
                va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some(i) = best {
            return i.get("file_path").and_then(|v| v.as_str()).map(String::from);
        }
    }
    None
}

/// The `n` best voted images of one language (None = text-free).
fn top_images(list: Option<&Value>, lang: Option<&str>, n: usize) -> Vec<String> {
    let mut items: Vec<&Value> = list
        .and_then(|l| l.as_array())
        .map(|a| a.iter().filter(|i| i.get("iso_639_1").and_then(|v| v.as_str()) == lang).collect())
        .unwrap_or_default();
    let votes = |i: &Value| i.get("vote_average").and_then(|v| v.as_f64()).unwrap_or(0.0);
    items.sort_by(|a, b| votes(b).partial_cmp(&votes(a)).unwrap_or(std::cmp::Ordering::Equal));
    items.iter().filter_map(|i| i.get("file_path").and_then(|v| v.as_str()).map(String::from)).take(n).collect()
}

/// Letters and digits only, so "Spider-Man: No Way Home" matches "Spider Man No Way Home".
fn title_key(s: &str) -> String {
    normalize_text(s).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Whether a TMDB result is the title we asked for (any of its names).
fn same_title(d: &Value, title: &str) -> bool {
    let want = title_key(title);
    if want.chars().count() < 2 {
        return false;
    }
    ["name", "title", "original_name", "original_title"]
        .iter()
        .filter_map(|k| d.get(*k).and_then(|v| v.as_str()))
        .map(title_key)
        .any(|got| got == want || (got.chars().count() >= 4 && want.chars().count() >= 4 && (got.contains(&want) || want.contains(&got))))
}

async fn search_id(client: &reqwest::Client, key: &str, endpoint: &str, title: &str, year: Option<i64>) -> Result<Option<i64>, String> {
    let url = format!("https://api.themoviedb.org/3/search/{}", endpoint);
    let mut query = vec![("query", title.to_string()), ("include_adult", "false".into())];
    if let Some(y) = year {
        query.push((if endpoint == "tv" { "first_air_date_year" } else { "year" }, y.to_string()));
    }
    let search = get_json(client, key, &url, &query).await?;
    let mut hit = search.get("results").and_then(|r| r.as_array()).and_then(|r| r.first()).cloned();
    if hit.is_none() && year.is_some() {
        // Provider years are sometimes off by one; retry without it
        let search = get_json(client, key, &url, &query[..2]).await?;
        hit = search.get("results").and_then(|r| r.as_array()).and_then(|r| r.first()).cloned();
    }
    Ok(hit.as_ref().and_then(|h| h.get("id")).and_then(|v| v.as_i64()))
}

async fn fetch_details(client: &reqwest::Client, key: &str, endpoint: &str, id: i64) -> Result<Value, String> {
    get_json(
        client,
        key,
        &format!("https://api.themoviedb.org/3/{}/{}", endpoint, id),
        &[
            ("append_to_response", "images,credits,videos".into()),
            ("include_image_language", "en,ar,null".into()),
        ],
    )
    .await
}

/// `tmdb_id`: the provider's own id, trusted. `hint`: a guessed id, used only when its title matches.
/// Otherwise the first search result for the title.
async fn lookup(
    key: &str,
    kind: &str,
    title: &str,
    year: Option<i64>,
    tmdb_id: Option<i64>,
    hint: Option<i64>,
) -> Result<Option<TmdbDetails>, String> {
    let client = client()?;
    let endpoint = if kind == "series" { "tv" } else { "movie" };

    if let Some(id) = tmdb_id {
        match fetch_details(&client, key, endpoint, id).await {
            Ok(d) => return Ok(Some(to_details(id, &d))),
            // A stale id from the provider; fall back to searching
            Err(e) if e.ends_with("404") => {}
            Err(e) => return Err(e),
        }
    }
    if let Some(id) = hint {
        if let Ok(d) = fetch_details(&client, key, endpoint, id).await {
            if same_title(&d, title) {
                return Ok(Some(to_details(id, &d)));
            }
        }
    }
    let Some(id) = search_id(&client, key, endpoint, title, year).await? else {
        return Ok(None);
    };
    let d = fetch_details(&client, key, endpoint, id).await?;
    Ok(Some(to_details(id, &d)))
}

fn to_details(id: i64, d: &Value) -> TmdbDetails {
    let images = d.get("images");
    // Text-free backdrops look best behind the title
    let backdrop = pick_image(images.and_then(|i| i.get("backdrops")), &[None, Some("en")])
        .or_else(|| d.get("backdrop_path").and_then(|v| v.as_str()).map(String::from));
    let logo = pick_image(images.and_then(|i| i.get("logos")), &[Some("en"), Some("ar"), None]);

    let cast = d
        .get("credits")
        .and_then(|c| c.get("cast"))
        .and_then(|c| c.as_array())
        .map(|people| {
            people
                .iter()
                .take(12)
                .filter_map(|p| {
                    Some(TmdbPerson {
                        name: p.get("name")?.as_str()?.to_string(),
                        character: p.get("character").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from),
                        photo: img(p.get("profile_path").and_then(|v| v.as_str()), "w185"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let trailer = d
        .get("videos")
        .and_then(|v| v.get("results"))
        .and_then(|r| r.as_array())
        .and_then(|vids| {
            vids.iter()
                .filter(|v| v.get("site").and_then(|s| s.as_str()) == Some("YouTube"))
                .find(|v| v.get("type").and_then(|t| t.as_str()) == Some("Trailer"))
                .or_else(|| vids.iter().find(|v| v.get("site").and_then(|s| s.as_str()) == Some("YouTube")))
        })
        .and_then(|v| v.get("key"))
        .and_then(|k| k.as_str())
        .map(String::from);

    let mut backdrops: Vec<String> = top_images(images.and_then(|i| i.get("backdrops")), None, 6)
        .iter()
        .filter_map(|p| img(Some(p), "w1280"))
        .collect();
    if backdrops.is_empty() {
        backdrops.extend(img(backdrop.as_deref(), "w1280"));
    }

    TmdbDetails {
        tmdb_id: id,
        backdrop: img(backdrop.as_deref(), "w1280"),
        backdrops,
        logo: img(logo.as_deref(), "w500"),
        poster: img(d.get("poster_path").and_then(|v| v.as_str()), "w500"),
        overview: d.get("overview").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from),
        rating: d.get("vote_average").and_then(|v| v.as_f64()).filter(|r| *r > 0.0),
        trailer,
        cast,
    }
}

/// Artwork and cast for a movie or series. None when no key is set or nothing matched.
/// `tmdb_id` (from the provider) skips the title search; `tmdb_hint` is tried when its title matches.
#[tauri::command]
pub async fn tmdb_details(
    kind: String,
    title: String,
    year: Option<i64>,
    tmdb_id: Option<i64>,
    tmdb_hint: Option<i64>,
    db: State<'_, Database>,
) -> Result<Option<TmdbDetails>, String> {
    let Some(key) = setting(&db, "tmdb_api_key").filter(|k| !k.trim().is_empty()) else {
        return Ok(None);
    };
    let title = title.trim();
    if title.is_empty() && tmdb_id.is_none() {
        return Ok(None);
    }
    let cache_key = match (tmdb_id, tmdb_hint) {
        (Some(id), _) => format!("tmdb_cache:{}:id:{}", kind, id),
        (None, Some(hint)) => format!("tmdb_cache:{}:{}:{}:hint:{}", kind, title.to_lowercase(), year.unwrap_or(0), hint),
        (None, None) => format!("tmdb_cache:{}:{}:{}", kind, title.to_lowercase(), year.unwrap_or(0)),
    };
    if let Some(cached) = setting(&db, &cache_key) {
        return Ok(serde_json::from_str::<Option<TmdbDetails>>(&cached).unwrap_or(None));
    }
    let found = lookup(key.trim(), &kind, title, year, tmdb_id, tmdb_hint).await?;
    if let Ok(json) = serde_json::to_string(&found) {
        save_setting(&db, &cache_key, &json);
    }
    Ok(found)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbEpisode {
    pub episode: i64,
    pub name: Option<String>,
    pub overview: Option<String>,
    /// Frame from the episode
    pub still: Option<String>,
    pub air_date: Option<String>,
    /// Minutes
    pub runtime: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct CachedSeason {
    at: i64,
    episodes: Vec<TmdbEpisode>,
}

// Running seasons gain episodes; refresh their names and stills every few days
const SEASON_CACHE_SECS: i64 = 3 * 24 * 3600;

fn parse_season(d: &Value) -> Vec<TmdbEpisode> {
    let text = |e: &Value, k: &str| e.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    d.get("episodes")
        .and_then(|e| e.as_array())
        .map(|eps| {
            eps.iter()
                .filter_map(|e| {
                    Some(TmdbEpisode {
                        episode: e.get("episode_number")?.as_i64()?,
                        name: text(e, "name"),
                        overview: text(e, "overview"),
                        still: img(e.get("still_path").and_then(|v| v.as_str()), "w300"),
                        air_date: text(e, "air_date"),
                        runtime: e.get("runtime").and_then(|v| v.as_i64()).filter(|m| *m > 0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Episode names, stills and summaries for one season of a TMDB series. Empty when no key is set.
#[tauri::command]
pub async fn tmdb_season(tv_id: i64, season: i64, db: State<'_, Database>) -> Result<Vec<TmdbEpisode>, String> {
    let Some(key) = setting(&db, "tmdb_api_key").filter(|k| !k.trim().is_empty()) else {
        return Ok(vec![]);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cache_key = format!("tmdb_season:{}:{}", tv_id, season);
    if let Some(cached) = setting(&db, &cache_key).and_then(|c| serde_json::from_str::<CachedSeason>(&c).ok()) {
        if now - cached.at < SEASON_CACHE_SECS {
            return Ok(cached.episodes);
        }
    }
    let url = format!("https://api.themoviedb.org/3/tv/{}/season/{}", tv_id, season);
    let episodes = match get_json(&client()?, key.trim(), &url, &[]).await {
        Ok(d) => parse_season(&d),
        // The provider's season numbering doesn't exist on TMDB
        Err(e) if e.ends_with("404") => vec![],
        Err(e) => return Err(e),
    };
    if let Ok(json) = serde_json::to_string(&CachedSeason { at: now, episodes: episodes.clone() }) {
        save_setting(&db, &cache_key, &json);
    }
    Ok(episodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_title_ignores_punctuation_and_checks_all_names() {
        let d = serde_json::json!({"name": "Tracker", "original_name": "Tracker"});
        assert!(same_title(&d, "Tracker"));
        assert!(!same_title(&d, "The Bear"));
        let d = serde_json::json!({"title": "Spider-Man: No Way Home", "original_title": "Spider-Man: No Way Home"});
        assert!(same_title(&d, "Spider Man No Way Home"));
        let d = serde_json::json!({"name": "Half Mother", "original_name": "Anne Yarısı"});
        assert!(same_title(&d, "Anne Yarısı"));
        // Too short to trust a partial match
        assert!(!same_title(&serde_json::json!({"name": "It"}), "Itaewon Class"));
    }

    #[test]
    fn top_images_picks_best_voted_text_free() {
        let list = serde_json::json!([
            {"file_path": "/en.jpg", "iso_639_1": "en", "vote_average": 9.0},
            {"file_path": "/a.jpg", "iso_639_1": null, "vote_average": 5.1},
            {"file_path": "/b.jpg", "iso_639_1": null, "vote_average": 5.6},
            {"file_path": "/c.jpg", "iso_639_1": null}
        ]);
        assert_eq!(top_images(Some(&list), None, 2), vec!["/b.jpg", "/a.jpg"]);
        assert_eq!(top_images(None, None, 2), Vec::<String>::new());
    }

    #[test]
    fn old_cache_entries_still_load() {
        let old = r#"{"tmdb_id":1,"backdrop":null,"logo":null,"poster":null,"overview":null,"rating":null,"trailer":null,"cast":[]}"#;
        let d: TmdbDetails = serde_json::from_str(old).unwrap();
        assert!(d.backdrops.is_empty());
    }

    #[test]
    fn parse_season_keeps_named_episodes_with_stills() {
        let d = serde_json::json!({"episodes": [
            {"episode_number": 1, "name": "Pilot", "overview": "", "still_path": "/a.jpg", "air_date": "2024-02-11", "runtime": 44},
            {"episode_number": 2, "name": "Klamath Falls", "still_path": null, "runtime": 0},
            {"name": "no number"}
        ]});
        let eps = parse_season(&d);
        assert_eq!(eps.len(), 2);
        assert_eq!(eps[0].still.as_deref(), Some("https://image.tmdb.org/t/p/w300/a.jpg"));
        assert_eq!(eps[0].overview, None);
        assert_eq!(eps[0].runtime, Some(44));
        assert_eq!((eps[1].still.clone(), eps[1].runtime), (None, None));
    }
}

/// Checks a TMDB key before saving it.
#[tauri::command]
pub async fn tmdb_check_key(key: String) -> Result<bool, String> {
    let client = client()?;
    match get_json(&client, key.trim(), "https://api.themoviedb.org/3/configuration", &[]).await {
        Ok(_) => Ok(true),
        Err(e) if e.contains("rejected") => Ok(false),
        Err(e) => Err(e),
    }
}
