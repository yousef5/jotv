use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalkerCredentials {
    pub server: String,
    pub mac_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalkerChannel {
    pub name: String,
    pub stream_url: String,
    pub logo_url: Option<String>,
    pub group: String,
}

#[derive(Debug, Deserialize)]
struct StalkerCategoryResponse {
    #[serde(default)]
    js: Vec<StalkerCategoryItem>,
}

#[derive(Debug, Deserialize)]
struct StalkerCategoryItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize)]
struct StalkerChannelResponse {
    #[serde(default)]
    js: StalkerChannelData,
}

#[derive(Debug, Default, Deserialize)]
struct StalkerChannelData {
    #[serde(default)]
    data: Vec<StalkerChannelItem>,
    #[serde(default)]
    total_items: i64,
}

#[derive(Debug, Deserialize)]
struct StalkerChannelItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    cmd: String,
    #[serde(default)]
    logo: String,
    #[serde(default)]
    tv_genre_id: String,
}

impl StalkerCredentials {
    /// Build the portal API URL for a given action.
    pub fn portal_url(&self, action: &str) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/stalker_portal/server/load.php?type=itv&action={}",
            server, action
        )
    }

    /// Build the required HTTP headers for Stalker portal requests.
    pub fn headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert(
            "Cookie".to_string(),
            format!("mac={}", self.mac_address),
        );
        headers.insert(
            "X-User-Agent".to_string(),
            "Model: MAG250; Link: WiFi".to_string(),
        );
        headers
    }
}

/// Fetch channels from a Stalker portal.
pub async fn fetch_stalker_channels(
    creds: &StalkerCredentials,
) -> Result<Vec<StalkerChannel>, String> {
    let client = reqwest::Client::new();
    let hdrs = creds.headers();

    let mut req_headers = reqwest::header::HeaderMap::new();
    for (k, v) in &hdrs {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(v),
        ) {
            req_headers.insert(name, value);
        }
    }

    // Fetch categories
    let cat_url = creds.portal_url("get_genres");
    let cat_resp: StalkerCategoryResponse = client
        .get(&cat_url)
        .headers(req_headers.clone())
        .send()
        .await
        .map_err(|e| format!("Failed to fetch Stalker categories: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse Stalker categories: {}", e))?;

    let cat_map: HashMap<String, String> = cat_resp
        .js
        .into_iter()
        .map(|c| (c.id, c.title))
        .collect();

    // Fetch channels with pagination
    let mut channels = Vec::new();
    let mut page = 1;

    loop {
        let ch_url = format!(
            "{}&sortby=name&p={}",
            creds.portal_url("get_ordered_list"),
            page
        );
        let ch_resp: StalkerChannelResponse = client
            .get(&ch_url)
            .headers(req_headers.clone())
            .send()
            .await
            .map_err(|e| format!("Failed to fetch Stalker channels page {}: {}", page, e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse Stalker channels page {}: {}", page, e))?;

        if ch_resp.js.data.is_empty() {
            break;
        }

        for item in &ch_resp.js.data {
            // Extract actual URL from the cmd field (format: "ffmpeg http://...")
            let stream_url = item
                .cmd
                .strip_prefix("ffmpeg ")
                .unwrap_or(&item.cmd)
                .to_string();

            let logo_url = if item.logo.is_empty() {
                None
            } else {
                Some(item.logo.clone())
            };

            let group = cat_map
                .get(&item.tv_genre_id)
                .cloned()
                .unwrap_or_default();

            channels.push(StalkerChannel {
                name: item.name.clone(),
                stream_url,
                logo_url,
                group,
            });
        }

        let fetched = channels.len() as i64;
        if fetched >= ch_resp.js.total_items {
            break;
        }
        page += 1;
    }

    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_creds() -> StalkerCredentials {
        StalkerCredentials {
            server: "http://portal.example.com".to_string(),
            mac_address: "00:1A:79:AA:BB:CC".to_string(),
        }
    }

    #[test]
    fn test_portal_url() {
        let creds = test_creds();
        assert_eq!(
            creds.portal_url("get_genres"),
            "http://portal.example.com/stalker_portal/server/load.php?type=itv&action=get_genres"
        );
    }

    #[test]
    fn test_headers() {
        let creds = test_creds();
        let hdrs = creds.headers();
        assert_eq!(hdrs.get("Cookie").unwrap(), "mac=00:1A:79:AA:BB:CC");
        assert_eq!(
            hdrs.get("X-User-Agent").unwrap(),
            "Model: MAG250; Link: WiFi"
        );
    }

    #[test]
    fn test_trailing_slash() {
        let creds = StalkerCredentials {
            server: "http://portal.example.com/".to_string(),
            mac_address: "00:1A:79:AA:BB:CC".to_string(),
        };
        assert_eq!(
            creds.portal_url("get_genres"),
            "http://portal.example.com/stalker_portal/server/load.php?type=itv&action=get_genres"
        );
    }
}
