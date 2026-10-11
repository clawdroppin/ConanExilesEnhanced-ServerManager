//! Steam Workshop metadata (game AppID 440900).
//!
//! * `ISteamRemoteStorage/GetPublishedFileDetails` and `GetCollectionDetails` work **without** an
//!   API key (verified) – titles, previews, sizes, update times and tags (`Enhanced` / `Legacy`).
//! * Dependencies ("Required items") need `IPublishedFileService/GetDetails`, which requires a
//!   Steam Web API key; without one the public item page is parsed instead.
//! * Search (`IPublishedFileService/QueryFiles`) requires the key too.

use crate::error::{AppError, AppResult};
use crate::models::GAME_APP_ID;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkshopItem {
    pub id: String,
    /// false when Steam doesn't know the id (deleted / private / wrong app).
    pub found: bool,
    pub title: String,
    pub preview_url: String,
    pub file_size: u64,
    pub time_created: i64,
    /// Unix seconds.
    pub time_updated: i64,
    pub tags: Vec<String>,
    pub subscriptions: u64,
    pub creator: String,
    pub banned: bool,
    /// The item belongs to Conan Exiles.
    pub for_conan: bool,
    pub description: String,
}

impl WorkshopItem {
    pub fn is_legacy(&self) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case("Legacy")) && !self.is_enhanced()
    }
    pub fn is_enhanced(&self) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case("Enhanced"))
    }
}

static ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:[?&]id=|^)(\d{6,12})(?:\D|$)").unwrap());

/// Accepts a raw id, a Workshop/collection URL, or `steam://url/CommunityFilePage/<id>`.
pub fn parse_id(input: &str) -> Option<String> {
    let t = input.trim();
    if let Some(rest) = t.rsplit("CommunityFilePage/").next().filter(|_| t.contains("CommunityFilePage/")) {
        return rest.trim_matches('/').chars().take_while(|c| c.is_ascii_digit()).collect::<String>().into();
    }
    ID_RE.captures(t).map(|c| c[1].to_string())
}

pub async fn details(http: &reqwest::Client, ids: &[String]) -> AppResult<Vec<WorkshopItem>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for chunk in ids.chunks(100) {
        let mut form: Vec<(String, String)> = vec![("itemcount".into(), chunk.len().to_string())];
        for (i, id) in chunk.iter().enumerate() {
            form.push((format!("publishedfileids[{i}]"), id.clone()));
        }
        let v: serde_json::Value = http
            .post("https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/")
            .form(&form)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let empty = vec![];
        for d in v["response"]["publishedfiledetails"].as_array().unwrap_or(&empty) {
            out.push(parse_details(d));
        }
    }
    Ok(out)
}

fn s(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn num(v: &serde_json::Value) -> i64 {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

pub fn parse_details(d: &serde_json::Value) -> WorkshopItem {
    let found = d["result"].as_i64() == Some(1);
    let desc = s(&d["description"]);
    WorkshopItem {
        id: s(&d["publishedfileid"]),
        found,
        title: s(&d["title"]),
        preview_url: s(&d["preview_url"]),
        file_size: num(&d["file_size"]) as u64,
        time_created: num(&d["time_created"]),
        time_updated: num(&d["time_updated"]),
        tags: d["tags"].as_array().map(|a| a.iter().map(|t| s(&t["tag"])).collect()).unwrap_or_default(),
        subscriptions: num(&d["subscriptions"]) as u64,
        creator: s(&d["creator"]),
        banned: d["banned"].as_i64().unwrap_or(0) != 0,
        for_conan: num(&d["consumer_app_id"]) == GAME_APP_ID as i64 || num(&d["creator_app_id"]) == GAME_APP_ID as i64,
        description: desc.chars().take(4000).collect(),
    }
}

/// Item ids in a Workshop collection (nested collections are expanded one level).
pub async fn collection(http: &reqwest::Client, id: &str) -> AppResult<Vec<String>> {
    async fn fetch(http: &reqwest::Client, id: &str) -> AppResult<Vec<(String, i64)>> {
        let v: serde_json::Value = http
            .post("https://api.steampowered.com/ISteamRemoteStorage/GetCollectionDetails/v1/")
            .form(&[("collectioncount", "1"), ("publishedfileids[0]", id)])
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let c = &v["response"]["collectiondetails"][0];
        if c["result"].as_i64() != Some(1) {
            return Err(AppError::msg(format!("{id} isn't a Workshop collection (or it's private).")));
        }
        Ok(c["children"]
            .as_array()
            .map(|a| a.iter().map(|ch| (s(&ch["publishedfileid"]), num(&ch["filetype"]))).collect())
            .unwrap_or_default())
    }
    let mut out = vec![];
    for (child, kind) in fetch(http, id).await? {
        if kind == 2 {
            if let Ok(inner) = fetch(http, &child).await {
                out.extend(inner.into_iter().filter(|(_, k)| *k != 2).map(|(c, _)| c));
            }
        } else {
            out.push(child);
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|x| seen.insert(x.clone()));
    Ok(out)
}

static REQUIRED_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)id="RequiredItems".*?</div>\s*</div>"#).unwrap());
static HREF_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"filedetails/\?id=(\d+)").unwrap());

/// Required items of a Workshop item. Uses the API when a key is set; otherwise parses the
/// public item page (`id="RequiredItems"` block – Steam's standard markup; unverified on a Conan
/// item because no tested Enhanced mod declared dependencies yet).
pub async fn dependencies(http: &reqwest::Client, api_key: &str, id: &str) -> AppResult<Vec<String>> {
    if !api_key.trim().is_empty() {
        let url = format!(
            "https://api.steampowered.com/IPublishedFileService/GetDetails/v1/?key={}&publishedfileids%5B0%5D={id}&includechildren=true",
            api_key.trim()
        );
        let v: serde_json::Value = http.get(url).timeout(std::time::Duration::from_secs(15)).send().await?.error_for_status()?.json().await?;
        let d = &v["response"]["publishedfiledetails"][0];
        return Ok(d["children"].as_array().map(|a| a.iter().map(|c| s(&c["publishedfileid"])).collect()).unwrap_or_default());
    }
    let html = http
        .get(format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}"))
        .header("Cookie", "birthtime=470682001; wants_mature_content=1")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .text()
        .await?;
    Ok(parse_required_items(&html, id))
}

pub fn parse_required_items(html: &str, own_id: &str) -> Vec<String> {
    let Some(m) = REQUIRED_BLOCK_RE.find(html) else { return vec![] };
    let mut out: Vec<String> = HREF_ID_RE.captures_iter(m.as_str()).map(|c| c[1].to_string()).filter(|i| i != own_id).collect();
    out.dedup();
    out
}

/// Workshop search (Enhanced-tagged items). Requires a Steam Web API key.
pub async fn search(http: &reqwest::Client, api_key: &str, text: &str, page: u32, enhanced_only: bool) -> AppResult<Vec<WorkshopItem>> {
    if api_key.trim().is_empty() {
        return Err(AppError::msg("Workshop search needs a free Steam Web API key (App settings → Steam). Adding mods by ID or URL works without one."));
    }
    let mut url = format!(
        "https://api.steampowered.com/IPublishedFileService/QueryFiles/v1/?key={}&appid={GAME_APP_ID}&numperpage=30&page={}&return_tags=true&return_previews=true&return_short_description=true&return_metadata=true",
        api_key.trim(),
        page.max(1)
    );
    if text.trim().is_empty() {
        url.push_str("&query_type=9"); // ranked by trend
    } else {
        url.push_str("&query_type=12&search_text=");
        url.push_str(&urlencode(text.trim()));
    }
    if enhanced_only {
        url.push_str("&requiredtags%5B0%5D=Enhanced");
    }
    let v: serde_json::Value = http.get(url).timeout(std::time::Duration::from_secs(20)).send().await?.error_for_status()?.json().await?;
    let empty = vec![];
    Ok(v["response"]["publishedfiledetails"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|d| {
            let mut w = parse_details(d);
            // QueryFiles reports result=1 implicitly.
            w.found = !w.id.is_empty();
            if w.description.is_empty() {
                w.description = s(&d["short_description"]);
            }
            w
        })
        .collect())
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_from_inputs() {
        assert_eq!(parse_id("3745387835").as_deref(), Some("3745387835"));
        assert_eq!(parse_id("https://steamcommunity.com/sharedfiles/filedetails/?id=3815570855&searchtext=").as_deref(), Some("3815570855"));
        assert_eq!(parse_id("https://steamcommunity.com/workshop/filedetails/?l=german&id=880454836").as_deref(), Some("880454836"));
        assert_eq!(parse_id("steam://url/CommunityFilePage/3814308883").as_deref(), Some("3814308883"));
        assert_eq!(parse_id("not a mod"), None);
    }

    #[test]
    fn details_parsing_real_shape() {
        let d = serde_json::json!({
            "publishedfileid": "880454836", "result": 1, "creator_app_id": 440900, "consumer_app_id": 440900,
            "file_size": 26668482, "title": "[Legacy] Pippi", "time_updated": 1712142752,
            "tags": [{"tag": "Admin"}, {"tag": "Legacy"}], "banned": 0, "subscriptions": 630768
        });
        let w = parse_details(&d);
        assert!(w.found && w.for_conan && w.is_legacy() && !w.is_enhanced());
        assert_eq!(w.file_size, 26668482);
        let missing = parse_details(&serde_json::json!({"publishedfileid": "1", "result": 9}));
        assert!(!missing.found);
    }

    #[test]
    fn required_items_block() {
        let html = r#"<div class="requiredItemsContainer" id="RequiredItems"><a href="https://steamcommunity.com/workshop/filedetails/?id=111"><div class="requiredItem">A</div></a><a href="https://steamcommunity.com/workshop/filedetails/?id=222"><div class="requiredItem">B</div>
				</a>
			</div>
		</div>"#;
        assert_eq!(parse_required_items(html, "999"), vec!["111", "222"]);
        assert!(parse_required_items("<html></html>", "1").is_empty());
    }
}
