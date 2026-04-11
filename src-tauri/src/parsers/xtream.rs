use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamCredentials {
    pub server: String,
    pub username: String,
    pub password: String,
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
    pub name: String,
    #[serde(default)]
    pub stream_id: i64,
    #[serde(default)]
    pub stream_icon: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub container_extension: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XtreamChannel {
    pub name: String,
    pub stream_id: i64,
    pub stream_url: String,
    pub logo_url: Option<String>,
    pub epg_id: Option<String>,
    pub group: String,
    pub is_vod: bool,
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

    /// Build a live stream HLS URL.
    pub fn live_stream_url(&self, stream_id: i64) -> String {
        let server = self.server.trim_end_matches('/');
        format!(
            "{}/live/{}/{}/{}.m3u8",
            server, self.username, self.password, stream_id
        )
    }
}

/// Fetch all channels (live + VOD) from an Xtream Codes server.
pub async fn fetch_xtream_channels(
    creds: &XtreamCredentials,
) -> Result<Vec<XtreamChannel>, String> {
    let client = reqwest::Client::new();
    let base = creds.base_url();

    // Fetch live categories
    let live_cats: Vec<XtreamCategory> = client
        .get(format!("{}&action=get_live_categories", base))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch live categories: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse live categories: {}", e))?;

    let cat_map: std::collections::HashMap<String, String> = live_cats
        .iter()
        .map(|c| (c.category_id.clone(), c.category_name.clone()))
        .collect();

    // Fetch live streams
    let live_streams: Vec<XtreamLiveStream> = client
        .get(format!("{}&action=get_live_streams", base))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch live streams: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse live streams: {}", e))?;

    let mut channels: Vec<XtreamChannel> = live_streams
        .into_iter()
        .map(|s| {
            let group = s
                .category_id
                .as_ref()
                .and_then(|cid| cat_map.get(cid))
                .cloned()
                .unwrap_or_default();
            XtreamChannel {
                name: s.name,
                stream_id: s.stream_id,
                stream_url: creds.live_stream_url(s.stream_id),
                logo_url: s.stream_icon,
                epg_id: s.epg_channel_id,
                group,
                is_vod: false,
            }
        })
        .collect();

    // Fetch VOD categories
    let vod_cats: Vec<XtreamCategory> = client
        .get(format!("{}&action=get_vod_categories", base))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch VOD categories: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse VOD categories: {}", e))?;

    let vod_cat_map: std::collections::HashMap<String, String> = vod_cats
        .iter()
        .map(|c| (c.category_id.clone(), c.category_name.clone()))
        .collect();

    // Fetch VOD streams
    let vod_streams: Vec<XtreamVodItem> = client
        .get(format!("{}&action=get_vod_streams", base))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch VOD streams: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse VOD streams: {}", e))?;

    for v in vod_streams {
        let ext = v.container_extension.as_deref().unwrap_or("mp4");
        let group = v
            .category_id
            .as_ref()
            .and_then(|cid| vod_cat_map.get(cid))
            .cloned()
            .unwrap_or_default();
        channels.push(XtreamChannel {
            name: v.name,
            stream_id: v.stream_id,
            stream_url: creds.stream_url(v.stream_id, ext),
            logo_url: v.stream_icon,
            epg_id: None,
            group,
            is_vod: true,
        });
    }

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
