use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamCredentials {
    pub server: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamAccountInfo {
    pub username: String,
    pub status: String,
    pub exp_date: Option<String>,
    pub is_trial: Option<String>,
    pub active_cons: Option<String>,
    pub max_connections: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamServerInfo {
    pub url: Option<String>,
    pub port: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XtreamAuthResponse {
    user_info: XtreamAccountInfo,
    server_info: Option<XtreamServerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamCategory {
    pub category_id: String,
    pub category_name: String,
    #[serde(default)]
    pub parent_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamLiveStream {
    #[serde(default)]
    pub num: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub stream_id: i64,
    #[serde(default)]
    pub stream_icon: Option<String>,
    #[serde(default)]
    pub epg_channel_id: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamVodItem {
    #[serde(default)]
    pub num: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub stream_id: i64,
    #[serde(default)]
    pub stream_icon: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub container_extension: Option<String>,
    #[serde(default)]
    pub added: Option<serde_json::Value>,
    #[serde(default)]
    pub rating: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamSeriesItem {
    #[serde(default)]
    pub series_id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub stream_icon: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub last_modified: Option<serde_json::Value>,
    #[serde(default)]
    pub rating: Option<serde_json::Value>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default, rename = "releaseDate")]
    pub release_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamSeriesInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub plot: Option<String>,
    #[serde(default)]
    pub cast: Option<String>,
    #[serde(default)]
    pub director: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default, alias = "releaseDate")]
    pub release_date: Option<String>,
    #[serde(default)]
    pub rating: Option<serde_json::Value>,
    #[serde(default)]
    pub backdrop_path: Option<serde_json::Value>,
}

/// VOD movie info from Xtream get_vod_info API.
/// Parsed manually because the API often returns duplicate JSON keys (e.g. "plot" twice).
#[derive(Debug, Clone, Serialize)]
pub struct XtreamVodInfo {
    pub name: Option<String>,
    pub cover: Option<String>,
    pub plot: Option<String>,
    pub cast: Option<String>,
    pub director: Option<String>,
    pub genre: Option<String>,
    pub release_date: Option<String>,
    pub rating: Option<serde_json::Value>,
    pub backdrop_path: Option<serde_json::Value>,
    pub duration: Option<String>,
    pub duration_secs: Option<i64>,
    pub youtube_trailer: Option<String>,
}

/// Helper to extract an optional string from a JSON object, trying multiple keys.
fn json_str(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(serde_json::Value::String(s)) = obj.get(*key) {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }
    None
}

impl XtreamVodInfo {
    fn from_value(val: &serde_json::Value) -> Self {
        let obj = val.as_object();
        let empty = serde_json::Map::new();
        let m = obj.unwrap_or(&empty);
        XtreamVodInfo {
            name: json_str(m, &["name"]),
            cover: json_str(m, &["cover_big", "movie_image", "stream_icon", "cover"]),
            plot: json_str(m, &["plot", "description"]),
            cast: json_str(m, &["cast"]),
            director: json_str(m, &["director"]),
            genre: json_str(m, &["genre"]),
            release_date: json_str(m, &["releasedate", "releaseDate", "release_date"]),
            rating: m.get("rating").cloned(),
            backdrop_path: m.get("backdrop_path").cloned(),
            duration: json_str(m, &["duration"]),
            duration_secs: m.get("duration_secs").and_then(|v| v.as_i64()),
            youtube_trailer: json_str(m, &["youtube_trailer"]),
        }
    }
}

/// Full VOD detail with info
#[derive(Debug, Clone, Serialize)]
pub struct VodDetail {
    pub info: XtreamVodInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamEpisode {
    #[serde(default, deserialize_with = "deserialize_id")]
    pub id: i64,
    #[serde(default)]
    pub episode_num: serde_json::Value,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub container_extension: Option<String>,
    #[serde(default)]
    pub season: serde_json::Value,
    #[serde(default)]
    pub direct_source: Option<String>,
    /// Object with movie_image / plot / duration_secs, or `[]` on some servers
    #[serde(default)]
    pub info: serde_json::Value,
}

/// Xtream APIs return IDs as either strings or numbers — handle both.
fn deserialize_id<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = serde_json::Value::deserialize(deserializer)?;
    match val {
        serde_json::Value::Number(n) => n.as_i64().ok_or_else(|| serde::de::Error::custom("invalid number")),
        serde_json::Value::String(s) => s.parse::<i64>().map_err(serde::de::Error::custom),
        _ => Ok(0),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XtreamSeriesInfoRaw {
    #[serde(default)]
    info: Option<XtreamSeriesInfo>,
    #[serde(default)]
    episodes: Option<serde_json::Value>,
}

/// Full series detail with info + episodes grouped by season
#[derive(Debug, Clone, Serialize)]
pub struct SeriesDetail {
    pub info: XtreamSeriesInfo,
    pub seasons: Vec<SeasonDetail>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeasonDetail {
    pub season_number: String,
    pub episodes: Vec<EpisodeDetail>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EpisodeDetail {
    pub id: i64,
    pub episode_num: String,
    pub title: String,
    pub stream_url: String,
    pub container_extension: String,
    pub image: Option<String>,
    pub plot: Option<String>,
    pub duration_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamChannel {
    pub name: String,
    pub stream_id: i64,
    pub stream_url: String,
    pub logo_url: Option<String>,
    pub epg_id: Option<String>,
    pub group: String,
    pub content_type: String,
    pub added_on_server: i64,
    pub rating: Option<f64>,
    pub year: Option<i64>,
    pub genre: Option<String>,
}

/// Rating as sent by Xtream: "6.2", 6.2, "" or null. 0 means "not rated".
fn parse_rating(val: &Option<serde_json::Value>) -> Option<f64> {
    let n = match val.as_ref()? {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    (n > 0.0 && n <= 10.0).then(|| (n * 10.0).round() / 10.0)
}

/// Year from a release date ("2024-05-01") or a title like "Name (2024)".
fn parse_year(release_date: Option<&str>, name: &str) -> Option<i64> {
    let from_date = release_date
        .and_then(|d| d.trim().get(0..4))
        .and_then(|y| y.parse::<i64>().ok());
    let from_name = || {
        let bytes = name.as_bytes();
        (0..bytes.len().saturating_sub(3)).rev().find_map(|i| {
            let y: i64 = name.get(i..i + 4)?.parse().ok()?;
            let before = i.checked_sub(1).map(|j| bytes[j]);
            let after = bytes.get(i + 4).copied();
            let boundary = |b: Option<u8>| b.map_or(true, |c| !c.is_ascii_digit());
            ((1900..=2100).contains(&y) && boundary(before) && boundary(after)).then_some(y)
        })
    };
    from_date.filter(|y| (1900..=2100).contains(y)).or_else(from_name)
}

/// Parse a timestamp that can be a string or number from Xtream API
fn parse_timestamp(val: &Option<serde_json::Value>) -> i64 {
    match val {
        Some(serde_json::Value::String(s)) => s.parse::<i64>().unwrap_or(0),
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
        _ => 0,
    }
}

impl XtreamCredentials {
    /// Build the base player_api.php URL with credentials.
    pub fn base_url(&self) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/player_api.php?username={}&password={}",
            server, self.username, self.password
        )
    }

    /// Build a direct stream URL for a given stream ID and extension.
    pub fn stream_url(&self, stream_id: i64, extension: &str) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/{}/{}/{}.{}",
            server, self.username, self.password, stream_id, extension
        )
    }

    /// Build a series info URL (for fetching series metadata).
    pub fn series_url(&self, series_id: i64) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/series/{}/{}/{}",
            server, self.username, self.password, series_id
        )
    }

    /// Build a series episode stream URL.
    pub fn series_episode_url(&self, episode_id: i64, extension: &str) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/series/{}/{}/{}.{}",
            server, self.username, self.password, episode_id, extension
        )
    }

    /// Build a VOD/movie stream URL.
    pub fn movie_url(&self, stream_id: i64, extension: &str) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/movie/{}/{}/{}.{}",
            server, self.username, self.password, stream_id, extension
        )
    }

    /// Build a live stream HLS URL.
    pub fn live_stream_url(&self, stream_id: i64) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/live/{}/{}/{}.m3u8",
            server, self.username, self.password, stream_id
        )
    }
}

/// Create an HTTP client with IPTV-compatible headers.
fn xtream_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .connect_timeout(std::time::Duration::from_secs(15))
        .user_agent("JEOTV/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// Fetch account info from an Xtream Codes server.
pub async fn fetch_xtream_account_info(
    creds: &XtreamCredentials,
) -> Result<XtreamAccountInfo, String> {
    let client = xtream_client(15)?;

    let resp: XtreamAuthResponse = client
        .get(creds.base_url())
        .send()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(resp.user_info)
}

/// Fetch series detail (info + episodes) from Xtream API.
pub async fn fetch_series_info(
    creds: &XtreamCredentials,
    series_id: i64,
) -> Result<SeriesDetail, String> {
    let client = xtream_client(30)?;

    let url = format!("{}&action=get_series_info&series_id={}", creds.base_url(), series_id);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch series info: {}", e))?;

    let status = resp.status();
    let body = resp.bytes().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if !status.is_success() {
        let preview = String::from_utf8_lossy(&body[..body.len().min(200)]);
        return Err(format!("Server returned HTTP {}: {}", status.as_u16(), preview));
    }

    if body.is_empty() {
        return Err("Server returned empty response".to_string());
    }

    let raw: XtreamSeriesInfoRaw = serde_json::from_slice(&body)
        .map_err(|e| {
            let preview = String::from_utf8_lossy(&body[..body.len().min(300)]);
            format!("JSON parse error: {} — Response: {}", e, preview)
        })?;

    let info = raw.info.unwrap_or(XtreamSeriesInfo {
        name: None, cover: None, plot: None, cast: None,
        director: None, genre: None, release_date: None,
        rating: None, backdrop_path: None,
    });

    let mut seasons: Vec<SeasonDetail> = Vec::new();
    if let Some(episodes_val) = raw.episodes {
        // episodes can be an object with string or int keys
        if let serde_json::Value::Object(map) = episodes_val {
            let mut season_keys: Vec<String> = map.keys().cloned().collect();
            season_keys.sort_by_key(|k| k.parse::<i64>().unwrap_or(0));

            for key in season_keys {
                if let Some(eps_val) = map.get(&key) {
                    // Parse episodes array
                    if let Ok(eps) = serde_json::from_value::<Vec<XtreamEpisode>>(eps_val.clone()) {
                        let episodes: Vec<EpisodeDetail> = eps.iter().map(|ep| {
                            let ext = ep.container_extension.as_deref().unwrap_or("mp4");
                            let ep_num = match &ep.episode_num {
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::String(s) => s.clone(),
                                _ => "0".to_string(),
                            };
                            let info = ep.info.as_object();
                            let text = |key: &str| {
                                info.and_then(|m| m.get(key))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                            };
                            EpisodeDetail {
                                id: ep.id,
                                episode_num: ep_num,
                                title: ep.title.clone(),
                                stream_url: creds.series_episode_url(ep.id, ext),
                                container_extension: ext.to_string(),
                                image: text("movie_image").filter(|s| s.starts_with("http")),
                                plot: text("plot"),
                                duration_secs: info
                                    .and_then(|m| m.get("duration_secs"))
                                    .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))),
                            }
                        }).collect();

                        if !episodes.is_empty() {
                            seasons.push(SeasonDetail {
                                season_number: key,
                                episodes,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(SeriesDetail { info, seasons })
}

/// Fetch VOD info from Xtream API.
pub async fn fetch_vod_info(
    creds: &XtreamCredentials,
    vod_id: i64,
) -> Result<VodDetail, String> {
    let client = xtream_client(30)?;

    let url = format!("{}&action=get_vod_info&vod_id={}", creds.base_url(), vod_id);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch VOD info: {}", e))?;

    let status = resp.status();
    let body = resp.bytes().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if !status.is_success() {
        let preview = String::from_utf8_lossy(&body[..body.len().min(200)]);
        return Err(format!("Server returned HTTP {}: {}", status.as_u16(), preview));
    }

    if body.is_empty() {
        return Err("Server returned empty response".to_string());
    }

    // Parse as raw Value to handle duplicate keys (common in Xtream APIs).
    // serde_json::Value silently takes the last value for duplicate keys.
    let raw: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| {
            let preview = String::from_utf8_lossy(&body[..body.len().min(300)]);
            format!("JSON parse error: {} — Response: {}", e, preview)
        })?;

    let info = if let Some(info_val) = raw.get("info") {
        XtreamVodInfo::from_value(info_val)
    } else {
        // Some APIs put info fields at root level
        XtreamVodInfo::from_value(&raw)
    };

    Ok(VodDetail { info })
}

/// Progress callback for reporting import status.
pub type ProgressCallback = Box<dyn Fn(&str, usize, usize) + Send + Sync>;

/// Helper: download response bytes
async fn download_bytes(resp: Result<reqwest::Response, reqwest::Error>) -> Option<Vec<u8>> {
    let r = resp.ok()?;
    let b = r.bytes().await.ok()?;
    Some(b.to_vec())
}

/// Helper: parse JSON on a blocking thread (avoids blocking the async runtime for large payloads)
fn parse_json_blocking<T: serde::de::DeserializeOwned + Send + 'static>(
    data: Vec<u8>,
) -> Result<T, String> {
    serde_json::from_slice(&data).map_err(|e| format!("JSON parse error: {}", e))
}

/// Fetch all channels (live + VOD + series) from an Xtream Codes server.
/// Downloads all 6 endpoints in parallel, parses the 3 large payloads on background threads.
pub async fn fetch_xtream_channels(
    creds: &XtreamCredentials,
    on_progress: Option<ProgressCallback>,
) -> Result<Vec<XtreamChannel>, String> {
    let client = xtream_client(180)?;
    let base = creds.base_url();

    let report = |stage: &str, current: usize, total: usize| {
        if let Some(ref cb) = on_progress {
            cb(stage, current, total);
        }
    };

    report("Downloading channel lists...", 0, 0);

    // Step 1: Send all 6 requests in parallel
    let (live_cats_r, live_streams_r, vod_cats_r, vod_streams_r, series_cats_r, series_r) = tokio::join!(
        client.get(format!("{}&action=get_live_categories", base)).send(),
        client.get(format!("{}&action=get_live_streams", base)).send(),
        client.get(format!("{}&action=get_vod_categories", base)).send(),
        client.get(format!("{}&action=get_vod_streams", base)).send(),
        client.get(format!("{}&action=get_series_categories", base)).send(),
        client.get(format!("{}&action=get_series", base)).send(),
    );

    report("Downloading response data...", 0, 0);

    // Step 2: Download ALL response bodies in parallel (this is the slow network part)
    let (live_cats_b, live_streams_b, vod_cats_b, vod_streams_b, series_cats_b, series_b) = tokio::join!(
        download_bytes(live_cats_r),
        download_bytes(live_streams_r),
        download_bytes(vod_cats_r),
        download_bytes(vod_streams_r),
        download_bytes(series_cats_r),
        download_bytes(series_r),
    );

    report("Parsing data...", 0, 0);

    // Step 3: Parse the 3 large payloads on background threads in parallel
    // Categories are small — parse inline
    let live_cat_map: std::collections::HashMap<String, String> = live_cats_b
        .and_then(|b| serde_json::from_slice::<Vec<XtreamCategory>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.category_id, c.category_name))
        .collect();

    let vod_cat_map: std::collections::HashMap<String, String> = vod_cats_b
        .and_then(|b| serde_json::from_slice::<Vec<XtreamCategory>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.category_id, c.category_name))
        .collect();

    let series_cat_map: std::collections::HashMap<String, String> = series_cats_b
        .and_then(|b| serde_json::from_slice::<Vec<XtreamCategory>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.category_id, c.category_name))
        .collect();

    // Parse the 3 large lists on blocking threads IN PARALLEL
    let live_handle = live_streams_b.map(|b| {
        tokio::task::spawn_blocking(move || parse_json_blocking::<Vec<XtreamLiveStream>>(b))
    });
    let vod_handle = vod_streams_b.map(|b| {
        tokio::task::spawn_blocking(move || parse_json_blocking::<Vec<XtreamVodItem>>(b))
    });
    let series_handle = series_b.map(|b| {
        tokio::task::spawn_blocking(move || parse_json_blocking::<Vec<XtreamSeriesItem>>(b))
    });

    // Await all 3 parse tasks in parallel
    let live_streams: Vec<XtreamLiveStream> = match live_handle {
        Some(h) => h.await.map_err(|e| e.to_string())?.unwrap_or_default(),
        None => return Err("Failed to download live streams".to_string()),
    };

    // VOD and series are optional
    let vod_streams: Vec<XtreamVodItem> = match vod_handle {
        Some(h) => h.await.map_err(|e| e.to_string())?.unwrap_or_default(),
        None => Vec::new(),
    };
    let series_items: Vec<XtreamSeriesItem> = match series_handle {
        Some(h) => h.await.map_err(|e| e.to_string())?.unwrap_or_default(),
        None => Vec::new(),
    };

    // Step 4: Build channel list
    let total_est = live_streams.len() + vod_streams.len() + series_items.len();
    let mut channels: Vec<XtreamChannel> = Vec::with_capacity(total_est);

    report(&format!("Processing {} live...", live_streams.len()), 0, total_est);
    channels.extend(live_streams.into_iter().map(|s| {
        let group = s.category_id.as_ref()
            .and_then(|cid| live_cat_map.get(cid)).cloned().unwrap_or_default();
        XtreamChannel {
            name: s.name, stream_id: s.stream_id,
            stream_url: creds.live_stream_url(s.stream_id),
            logo_url: s.stream_icon, epg_id: s.epg_channel_id,
            group, content_type: "live".to_string(),
            added_on_server: 0,
            rating: None, year: None, genre: None,
        }
    }));

    report(&format!("Processing {} VOD...", vod_streams.len()), channels.len(), total_est);
    channels.extend(vod_streams.into_iter().map(|v| {
        let ext = v.container_extension.as_deref().unwrap_or("mp4");
        let added = parse_timestamp(&v.added);
        let group = v.category_id.as_ref()
            .and_then(|cid| vod_cat_map.get(cid)).cloned().unwrap_or_default();
        let year = parse_year(None, &v.name);
        XtreamChannel {
            rating: parse_rating(&v.rating),
            year,
            genre: None,
            name: v.name, stream_id: v.stream_id,
            stream_url: creds.movie_url(v.stream_id, ext),
            logo_url: v.stream_icon, epg_id: None,
            group, content_type: "vod".to_string(),
            added_on_server: added,
        }
    }));

    report(&format!("Processing {} series...", series_items.len()), channels.len(), total_est);
    channels.extend(series_items.into_iter().map(|s| {
        let modified = parse_timestamp(&s.last_modified);
        let group = s.category_id.as_ref()
            .and_then(|cid| series_cat_map.get(cid)).cloned().unwrap_or_default();
        let logo = s.cover.or(s.stream_icon);
        let year = parse_year(s.release_date.as_deref(), &s.name);
        let genre = s.genre.as_deref().map(str::trim).filter(|g| !g.is_empty()).map(String::from);
        XtreamChannel {
            rating: parse_rating(&s.rating),
            year,
            genre,
            name: s.name, stream_id: s.series_id,
            stream_url: creds.series_url(s.series_id),
            logo_url: logo, epg_id: None,
            group, content_type: "series".to_string(),
            added_on_server: modified,
        }
    }));

    report("Fetch complete", channels.len(), channels.len());
    Ok(channels)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn test_creds() -> XtreamCredentials {
        XtreamCredentials {
            server: "http://example.com:8080".to_string(),
            username: "user1".to_string(),
            password: "pass1".to_string(),
        }
    }

    #[test]
    fn test_parse_rating() {
        assert_eq!(parse_rating(&Some(serde_json::json!("6.2"))), Some(6.2));
        assert_eq!(parse_rating(&Some(serde_json::json!(8))), Some(8.0));
        assert_eq!(parse_rating(&Some(serde_json::json!(""))), None);
        assert_eq!(parse_rating(&Some(serde_json::json!("0"))), None);
        assert_eq!(parse_rating(&None), None);
    }

    #[test]
    fn test_parse_year() {
        assert_eq!(parse_year(None, "Fall 2: Deadpoint (2026)"), Some(2026));
        assert_eq!(parse_year(None, "Stranglehold ( 2026 )"), Some(2026));
        assert_eq!(parse_year(Some("2019-03-01"), "Name"), Some(2019));
        assert_eq!(parse_year(Some(""), "1917 (2019) 4K"), Some(2019));
        assert_eq!(parse_year(None, "Room 12345"), None);
        assert_eq!(parse_year(None, "No year"), None);
    }

    #[test]
    fn test_base_url() {
        let creds = test_creds();
        assert_eq!(
            creds.base_url(),
            "http://example.com:8080/player_api.php?username=user1&password=pass1"
        );
    }

    #[test]
    fn test_live_stream_url() {
        let creds = test_creds();
        assert_eq!(
            creds.live_stream_url(42),
            "http://example.com:8080/live/user1/pass1/42.m3u8"
        );
    }

    #[test]
    fn test_stream_url() {
        let creds = test_creds();
        assert_eq!(
            creds.stream_url(99, "mp4"),
            "http://example.com:8080/user1/pass1/99.mp4"
        );
    }

    #[test]
    fn test_trailing_slash_handling() {
        let creds = XtreamCredentials {
            server: "http://example.com:8080/".to_string(),
            username: "user1".to_string(),
            password: "pass1".to_string(),
        };
        assert_eq!(
            creds.base_url(),
            "http://example.com:8080/player_api.php?username=user1&password=pass1"
        );
        assert_eq!(
            creds.live_stream_url(1),
            "http://example.com:8080/live/user1/pass1/1.m3u8"
        );
        assert_eq!(
            creds.stream_url(1, "ts"),
            "http://example.com:8080/user1/pass1/1.ts"
        );
    }
}
