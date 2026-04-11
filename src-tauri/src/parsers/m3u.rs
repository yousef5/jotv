use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct M3uEntry {
    pub name: String,
    pub group: String,
    pub logo_url: Option<String>,
    pub stream_url: String,
    pub epg_id: Option<String>,
    pub is_vod: bool,
}

/// Extract an attribute value from an #EXTINF line.
/// Looks for `key="value"` patterns.
fn extract_attribute(line: &str, key: &str) -> Option<String> {
    let pattern = format!("{}=\"", key);
    if let Some(start) = line.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = line[value_start..].find('"') {
            let value = line[value_start..value_start + end].to_string();
            if value.is_empty() {
                return None;
            }
            return Some(value);
        }
    }
    None
}

/// Extract the channel name from an #EXTINF line (text after the last comma).
fn extract_name(line: &str) -> String {
    if let Some(pos) = line.rfind(',') {
        line[pos + 1..].trim().to_string()
    } else {
        String::new()
    }
}

/// Detect whether a stream URL represents VOD content.
fn is_vod_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.ends_with(".mp4")
        || lower.ends_with(".mkv")
        || lower.ends_with(".avi")
        || lower.contains("/movie/")
        || lower.contains("/series/")
}

/// Parse an M3U/M3U8 playlist string into a list of entries.
pub fn parse_m3u(content: &str) -> Vec<M3uEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    // Skip #EXTM3U header if present
    if !lines.is_empty() && lines[0].trim().starts_with("#EXTM3U") {
        i = 1;
    }

    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with("#EXTINF") {
            let name = extract_name(line);
            let group = extract_attribute(line, "group-title").unwrap_or_default();
            let logo_url = extract_attribute(line, "tvg-logo");
            let epg_id = extract_attribute(line, "tvg-id");

            // Find next non-empty, non-comment line as the stream URL
            i += 1;
            while i < lines.len() {
                let next = lines[i].trim();
                if !next.is_empty() && !next.starts_with('#') {
                    let stream_url = next.to_string();
                    let is_vod = is_vod_url(&stream_url);
                    entries.push(M3uEntry {
                        name,
                        group,
                        logo_url,
                        stream_url,
                        epg_id,
                        is_vod,
                    });
                    break;
                }
                i += 1;
            }
        }

        i += 1;
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_m3u() {
        let content = r#"#EXTM3U
#EXTINF:-1 tvg-id="ch1.example" tvg-logo="http://logo.com/1.png" group-title="News",Channel One
http://stream.example.com/live/ch1.m3u8
#EXTINF:-1 tvg-id="ch2.example" tvg-logo="http://logo.com/2.png" group-title="Sports",Channel Two
http://stream.example.com/live/ch2.ts
"#;
        let entries = parse_m3u(content);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].name, "Channel One");
        assert_eq!(entries[0].group, "News");
        assert_eq!(entries[0].logo_url, Some("http://logo.com/1.png".to_string()));
        assert_eq!(entries[0].stream_url, "http://stream.example.com/live/ch1.m3u8");
        assert_eq!(entries[0].epg_id, Some("ch1.example".to_string()));
        assert!(!entries[0].is_vod);

        assert_eq!(entries[1].name, "Channel Two");
        assert_eq!(entries[1].group, "Sports");
        assert!(!entries[1].is_vod);
    }

    #[test]
    fn test_vod_detection() {
        let content = r#"#EXTM3U
#EXTINF:-1 group-title="Movies",Some Movie
http://server.com/movie/1234.mp4
#EXTINF:-1 group-title="Series",Some Series
http://server.com/series/5678.mkv
#EXTINF:-1 group-title="Live",Live Channel
http://server.com/live/stream.m3u8
"#;
        let entries = parse_m3u(content);
        assert_eq!(entries.len(), 3);

        assert!(entries[0].is_vod, "MP4 movie should be VOD");
        assert!(entries[1].is_vod, "MKV series should be VOD");
        assert!(!entries[2].is_vod, "M3U8 live should not be VOD");
    }

    #[test]
    fn test_empty_content() {
        let entries = parse_m3u("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_no_header() {
        let content = r#"#EXTINF:-1 group-title="Test",Test Channel
http://example.com/stream.ts
"#;
        let entries = parse_m3u(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Test Channel");
    }

    #[test]
    fn test_attribute_extraction() {
        let line = r#"#EXTINF:-1 tvg-id="abc.123" tvg-logo="http://img.com/logo.png" group-title="Entertainment",My Channel"#;
        assert_eq!(extract_attribute(line, "tvg-id"), Some("abc.123".to_string()));
        assert_eq!(extract_attribute(line, "tvg-logo"), Some("http://img.com/logo.png".to_string()));
        assert_eq!(extract_attribute(line, "group-title"), Some("Entertainment".to_string()));
        assert_eq!(extract_attribute(line, "nonexistent"), None);
    }
}
