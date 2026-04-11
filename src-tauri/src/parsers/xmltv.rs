use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpgProgram {
    pub channel_id: String,
    pub title: String,
    pub description: Option<String>,
    pub start: String,
    pub stop: String,
    pub category: Option<String>,
}

/// Convert XMLTV time format "20210315180000 +0000" to ISO-like "2021-03-15T18:00:00".
/// Only the datetime portion is parsed; the timezone offset is ignored.
pub fn parse_xmltv_time(raw: &str) -> String {
    let trimmed = raw.trim();
    // Take at least the first 14 characters (YYYYMMDDHHmmss)
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 14 {
        return trimmed.to_string();
    }
    format!(
        "{}-{}-{}T{}:{}:{}",
        &digits[0..4],
        &digits[4..6],
        &digits[6..8],
        &digits[8..10],
        &digits[10..12],
        &digits[12..14],
    )
}

/// Parse XMLTV formatted XML into a list of EPG programs.
pub fn parse_xmltv(xml: &str) -> Vec<EpgProgram> {
    let mut programs = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();

    // State for current programme being parsed
    let mut in_programme = false;
    let mut channel_id = String::new();
    let mut start = String::new();
    let mut stop = String::new();
    let mut title = String::new();
    let mut description: Option<String> = None;
    let mut category: Option<String> = None;
    let mut current_tag = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag_name == "programme" {
                    in_programme = true;
                    title.clear();
                    description = None;
                    category = None;
                    channel_id = String::new();
                    start = String::new();
                    stop = String::new();

                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        match key.as_str() {
                            "channel" => channel_id = val,
                            "start" => start = parse_xmltv_time(&val),
                            "stop" => stop = parse_xmltv_time(&val),
                            _ => {}
                        }
                    }
                }
                if in_programme {
                    current_tag = tag_name;
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_programme {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match current_tag.as_str() {
                        "title" => title = text,
                        "desc" => description = Some(text),
                        "category" => category = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag_name == "programme" && in_programme {
                    programs.push(EpgProgram {
                        channel_id: channel_id.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        start: start.clone(),
                        stop: stop.clone(),
                        category: category.clone(),
                    });
                    in_programme = false;
                }
                if in_programme {
                    current_tag.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    programs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_two_programmes() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tv>
  <programme start="20210315180000 +0000" stop="20210315190000 +0000" channel="ch1.example">
    <title>Evening News</title>
    <desc>Daily news broadcast</desc>
    <category>News</category>
  </programme>
  <programme start="20210315190000 +0000" stop="20210315200000 +0000" channel="ch2.example">
    <title>Sports Hour</title>
    <desc>Live sports coverage</desc>
    <category>Sports</category>
  </programme>
</tv>"#;

        let programs = parse_xmltv(xml);
        assert_eq!(programs.len(), 2);

        assert_eq!(programs[0].channel_id, "ch1.example");
        assert_eq!(programs[0].title, "Evening News");
        assert_eq!(programs[0].description, Some("Daily news broadcast".to_string()));
        assert_eq!(programs[0].start, "2021-03-15T18:00:00");
        assert_eq!(programs[0].stop, "2021-03-15T19:00:00");
        assert_eq!(programs[0].category, Some("News".to_string()));

        assert_eq!(programs[1].channel_id, "ch2.example");
        assert_eq!(programs[1].title, "Sports Hour");
        assert_eq!(programs[1].category, Some("Sports".to_string()));
    }

    #[test]
    fn test_empty_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><tv></tv>"#;
        let programs = parse_xmltv(xml);
        assert!(programs.is_empty());
    }

    #[test]
    fn test_parse_xmltv_time() {
        assert_eq!(
            parse_xmltv_time("20210315180000 +0000"),
            "2021-03-15T18:00:00"
        );
        assert_eq!(
            parse_xmltv_time("20231225235959 +0300"),
            "2023-12-25T23:59:59"
        );
    }
}
