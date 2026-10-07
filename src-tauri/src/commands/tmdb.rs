//! Optional artwork from TMDB (backdrops, title logos, cast photos, trailers).
//! Needs the user's own free TMDB key, stored in settings as `tmdb_api_key`.
//! Lookups are cached in the settings table, including "not found".

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

async fn lookup(key: &str, kind: &str, title: &str, year: Option<i64>) -> Result<Option<TmdbDetails>, String> {
    let client = client()?;
    let endpoint = if kind == "series" { "tv" } else { "movie" };

    let mut query = vec![("query", title.to_string()), ("include_adult", "false".into())];
    if let Some(y) = year {
        query.push((if endpoint == "tv" { "first_air_date_year" } else { "year" }, y.to_string()));
    }
    let search = get_json(&client, key, &format!("https://api.themoviedb.org/3/search/{}", endpoint), &query).await?;
    let mut hit = search.get("results").and_then(|r| r.as_array()).and_then(|r| r.first()).cloned();
    if hit.is_none() && year.is_some() {
        // Provider years are sometimes off by one; retry without it
        let search = get_json(&client, key, &format!("https://api.themoviedb.org/3/search/{}", endpoint), &query[..2]).await?;
        hit = search.get("results").and_then(|r| r.as_array()).and_then(|r| r.first()).cloned();
    }
    let Some(id) = hit.as_ref().and_then(|h| h.get("id")).and_then(|v| v.as_i64()) else {
        return Ok(None);
    };

    let d = get_json(
        &client,
        key,
        &format!("https://api.themoviedb.org/3/{}/{}", endpoint, id),
        &[
            ("append_to_response", "images,credits,videos".into()),
            ("include_image_language", "en,ar,null".into()),
        ],
    )
    .await?;

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

    Ok(Some(TmdbDetails {
        tmdb_id: id,
        backdrop: img(backdrop.as_deref(), "w1280"),
        logo: img(logo.as_deref(), "w500"),
        poster: img(d.get("poster_path").and_then(|v| v.as_str()), "w500"),
        overview: d.get("overview").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from),
        rating: d.get("vote_average").and_then(|v| v.as_f64()).filter(|r| *r > 0.0),
        trailer,
        cast,
    }))
}

/// Artwork and cast for a movie or series. None when no key is set or nothing matched.
#[tauri::command]
pub async fn tmdb_details(
    kind: String,
    title: String,
    year: Option<i64>,
    db: State<'_, Database>,
) -> Result<Option<TmdbDetails>, String> {
    let Some(key) = setting(&db, "tmdb_api_key").filter(|k| !k.trim().is_empty()) else {
        return Ok(None);
    };
    let title = title.trim();
    if title.is_empty() {
        return Ok(None);
    }
    let cache_key = format!("tmdb_cache:{}:{}:{}", kind, title.to_lowercase(), year.unwrap_or(0));
    if let Some(cached) = setting(&db, &cache_key) {
        return Ok(serde_json::from_str::<Option<TmdbDetails>>(&cached).unwrap_or(None));
    }
    let found = lookup(key.trim(), &kind, title, year).await?;
    if let Ok(json) = serde_json::to_string(&found) {
        save_setting(&db, &cache_key, &json);
    }
    Ok(found)
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
