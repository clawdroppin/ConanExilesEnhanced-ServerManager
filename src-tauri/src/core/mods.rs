//! Mod management for Conan Exiles Enhanced.
//!
//! Verified behaviour (AGENTS.md §3):
//! * `ConanSandbox\Mods\modlist.txt` lists **absolute** `.pak` paths, one per line; the server
//!   mounts them in file order (`Order=1000, 1001, …`), so file order = load order.
//! * Workshop paks are mounted straight from SteamCMD's download folder – no copying, and the
//!   file name stays exactly as Workshop ships it (a renamed pak silently fails to load).
//! * Each Enhanced pak embeds an uncompressed `modinfo.json` with `folderName`,
//!   `devkitRevisionNumber` (must be accepted by the server: `SetCompatibleDevkitVersions`),
//!   `minimumVersion` and `requestedHttpOrigins`.
//! * `ServerSettings.ini ServerModList` carries the Workshop ids so clients can auto-subscribe.
//!
//! The manager owns `modlist.txt`: it's regenerated from the profile on every start.

use super::ini::{self, IniDoc};
use super::steamcmd;
use super::workshop::WorkshopItem;
use crate::error::{AppError, AppResult};
use crate::models::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HttpOrigin {
    pub origin: String,
    pub reason: String,
}

/// The parts of a pak's embedded `modinfo.json` we use.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModInfo {
    pub name: String,
    pub description: String,
    pub change_note: String,
    pub author: String,
    pub author_url: String,
    pub version: String,
    pub folder_name: String,
    pub devkit_revision: Option<u32>,
    pub minimum_version: String,
    pub workshop_id: String,
    pub requested_http_origins: Vec<HttpOrigin>,
}

const KEY_MARK: &[u8] = b"\"devkitRevisionNumber\"";

/// Find and parse `modinfo.json` inside a pak without reading the whole file into memory: the
/// file is scanned in overlapping 4 MB windows from the end (the JSON sits near the index).
pub fn read_modinfo(pak: &Path) -> Option<ModInfo> {
    let mut f = std::fs::File::open(pak).ok()?;
    let len = f.metadata().ok()?.len();
    const WIN: u64 = 4 * 1024 * 1024;
    const OVERLAP: u64 = 256 * 1024;
    let mut end = len;
    loop {
        let start = end.saturating_sub(WIN);
        let mut buf = vec![0u8; (end - start) as usize];
        f.seek(SeekFrom::Start(start)).ok()?;
        f.read_exact(&mut buf).ok()?;
        if let Some(info) = find_modinfo(&buf) {
            return Some(info);
        }
        if start == 0 {
            return None;
        }
        end = (start + OVERLAP).min(len);
        if end <= OVERLAP {
            return None;
        }
    }
}

/// Locate a JSON object containing `"devkitRevisionNumber"` in a byte buffer.
pub fn find_modinfo(buf: &[u8]) -> Option<ModInfo> {
    let mut search_from = 0;
    while let Some(rel) = find_bytes(&buf[search_from..], KEY_MARK) {
        let at = search_from + rel;
        // Walk back to each candidate '{' (nearest first) and try to parse a balanced object.
        let lo = at.saturating_sub(256 * 1024);
        let mut i = at;
        while i > lo {
            i -= 1;
            if buf[i] == b'{' {
                if let Some(end) = balanced_end(buf, i) {
                    if end > at {
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf[i..end]) {
                            if v.get("devkitRevisionNumber").is_some() {
                                return Some(modinfo_from_json(&v));
                            }
                        }
                    }
                }
            }
        }
        search_from = at + KEY_MARK.len();
    }
    None
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// End (exclusive) of the JSON object starting at `start`, honouring strings and escapes.
fn balanced_end(buf: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (j, &b) in buf.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            0 => return None, // binary data – not our JSON
            _ => {}
        }
        if j - start > 512 * 1024 {
            return None;
        }
    }
    None
}

fn js(v: &serde_json::Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string()
}

pub fn modinfo_from_json(v: &serde_json::Value) -> ModInfo {
    let n = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
    let origins = v
        .get("requestedHttpOrigins")
        .and_then(|o| o.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| match e {
                    serde_json::Value::String(s) => Some(HttpOrigin { origin: s.clone(), reason: String::new() }),
                    serde_json::Value::Object(_) => {
                        let origin = ["origin", "Origin", "url", "Url"].iter().map(|k| js(e, k)).find(|s| !s.is_empty())?;
                        let reason = ["reason", "Reason", "description"].iter().map(|k| js(e, k)).find(|s| !s.is_empty()).unwrap_or_default();
                        Some(HttpOrigin { origin, reason })
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let ws = v
        .get("steamWorkshopFileIds")
        .and_then(|w| w.get("mainClient"))
        .and_then(|x| x.as_str().map(|s| s.to_string()).or_else(|| x.as_i64().map(|n| n.to_string())))
        .unwrap_or_default();
    ModInfo {
        name: js(v, "name"),
        description: js(v, "description").chars().take(4000).collect(),
        change_note: js(v, "changeNote").chars().take(2000).collect(),
        author: js(v, "author"),
        author_url: js(v, "authorUrl"),
        version: format!("{}.{}.{}", n("versionMajor"), n("versionMinor"), n("versionBuild")),
        folder_name: js(v, "folderName"),
        devkit_revision: v.get("devkitRevisionNumber").and_then(|x| x.as_u64()).map(|x| x as u32),
        minimum_version: js(v, "minimumVersion"),
        workshop_id: ws,
        requested_http_origins: origins,
    }
}

// ------------------------------------------------------------------------------- resolution

/// The pak SteamCMD downloaded for a Workshop item (first `.pak` in its folder).
pub fn workshop_pak(cache: &Path, id: &str) -> Option<PathBuf> {
    let dir = steamcmd::workshop_item_dir(cache, id);
    let mut paks: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("pak")).unwrap_or(false))
        .collect();
    paks.sort();
    paks.into_iter().next()
}

pub fn resolve_pak(cache: &Path, m: &ModEntry) -> Option<PathBuf> {
    if m.workshop_id.is_empty() {
        m.local_path.clone().filter(|p| p.exists())
    } else {
        workshop_pak(cache, &m.workshop_id)
    }
}

/// Lines for modlist.txt: absolute pak paths of enabled mods, in load order.
pub fn modlist_lines(cache: &Path, inst: &ServerInstance) -> (Vec<String>, Vec<String>) {
    let mut lines = vec![];
    let mut missing = vec![];
    for m in inst.mods.iter().filter(|m| m.enabled) {
        match resolve_pak(cache, m) {
            Some(p) => lines.push(p.display().to_string()),
            None => missing.push(if m.name.is_empty() { m.key() } else { m.name.clone() }),
        }
    }
    (lines, missing)
}

/// Write the manager-owned modlist.txt (CRLF, no BOM). Returns names of enabled mods that have no
/// pak on disk (they're left out so the server still starts).
pub fn write_modlist(cache: &Path, inst: &ServerInstance) -> AppResult<Vec<String>> {
    let (lines, missing) = modlist_lines(cache, inst);
    let path = inst.modlist_path();
    std::fs::create_dir_all(inst.mods_dir())?;
    let text = if lines.is_empty() { String::new() } else { lines.join("\r\n") + "\r\n" };
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        ini::write_file(&path, &text)?;
    }
    Ok(missing)
}

/// `ServerModList` value: enabled Workshop ids in load order.
pub fn server_mod_list(inst: &ServerInstance) -> String {
    inst.mods.iter().filter(|m| m.enabled && !m.workshop_id.is_empty()).map(|m| m.workshop_id.clone()).collect::<Vec<_>>().join(",")
}

/// Turn an existing modlist.txt (e.g. from an imported install) into profile entries.
pub fn parse_modlist(text: &str, mods_dir: &Path) -> Vec<ModEntry> {
    let ws_re = regex::Regex::new(r"(?i)[\\/]440900[\\/](\d+)[\\/]").unwrap();
    text.lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}'))
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
        .map(|l| {
            if let Some(c) = ws_re.captures(l) {
                return ModEntry { workshop_id: c[1].to_string(), enabled: true, ..Default::default() };
            }
            // Legacy relative form: `*ModName.pak` or `*\ConanSandbox\Mods\X.pak` (relative to Mods).
            let rel = l.trim_start_matches('*').trim_start_matches(['\\', '/']);
            let p = PathBuf::from(rel);
            let abs = if p.is_absolute() { p } else { mods_dir.join(Path::new(rel).file_name().unwrap_or_default()) };
            let name = abs.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            ModEntry { workshop_id: String::new(), local_path: Some(abs), enabled: true, name }
        })
        .collect()
}

// ----------------------------------------------------------------------------------- status

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModIssue {
    /// error | warn | info
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModView {
    pub entry: ModEntry,
    pub key: String,
    pub pak: Option<PathBuf>,
    pub size: u64,
    pub info: Option<ModInfo>,
    pub workshop: Option<WorkshopItem>,
    /// Unix seconds of the copy on disk (Workshop items).
    pub local_updated: Option<i64>,
    pub update_available: bool,
    /// Mounted by the running server this session.
    pub loaded: bool,
    pub issues: Vec<ModIssue>,
    /// 0–100: compatibility, freshness and load state rolled into one number.
    pub health: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModConflict {
    pub row: String,
    /// Mods overriding the row, in load order – the last one wins.
    pub mods: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequestView {
    pub mod_folder: String,
    pub origin: String,
    pub reason: String,
    /// allowed | denied | pending
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModsView {
    pub mods: Vec<ModView>,
    pub server_devkit: u32,
    pub conflicts: Vec<ModConflict>,
    pub http: Vec<HttpRequestView>,
    pub workshop_dir: PathBuf,
    pub folder_exists: bool,
    pub mods_path: PathBuf,
}

pub struct ModContext<'a> {
    pub cache: &'a Path,
    pub workshop: &'a HashMap<String, WorkshopItem>,
    pub server_devkit: u32,
    pub loaded: &'a [String],
    pub row_overrides: &'a BTreeMap<String, Vec<String>>,
    pub log_http: &'a [(String, String, String)],
}

pub fn build_view(inst: &ServerInstance, cx: &ModContext) -> ModsView {
    let local_times = steamcmd::workshop_local_times(cx.cache);
    let http_cfg = read_modhttp(inst);
    let mut mods = vec![];
    for m in &inst.mods {
        let pak = resolve_pak(cx.cache, m);
        let size = pak.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|x| x.len()).unwrap_or(0);
        let info = pak.as_ref().and_then(|p| read_modinfo(p));
        let ws = (!m.workshop_id.is_empty()).then(|| cx.workshop.get(&m.workshop_id).cloned()).flatten();
        let local_updated = local_times.get(&m.workshop_id).copied();
        let update_available = matches!((&ws, local_updated), (Some(w), Some(l)) if w.found && w.time_updated > l);
        let folder = info.as_ref().map(|i| i.folder_name.clone()).filter(|f| !f.is_empty()).or_else(|| pak.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned())).unwrap_or_default();
        let loaded = !folder.is_empty() && cx.loaded.iter().any(|l| l.eq_ignore_ascii_case(&folder));
        let mut issues = vec![];
        let mut push = |level: &str, msg: String| issues.push(ModIssue { level: level.into(), message: msg });

        if pak.is_none() {
            if m.workshop_id.is_empty() {
                push("error", "The pak file no longer exists at its path.".into());
            } else {
                push("error", "Not downloaded yet – it's fetched automatically on the next start (or click Update).".into());
            }
        }
        if let Some(w) = &ws {
            if !w.found {
                push("error", "Steam doesn't know this item any more (deleted, private or wrong id).".into());
            } else {
                if !w.for_conan {
                    push("error", "This Workshop item isn't a Conan Exiles mod.".into());
                }
                if inst.is_legacy() && w.is_enhanced() && !w.is_legacy() {
                    push("error", "Enhanced (UE5) mod on a Legacy (UE4) server – it won't load.".into());
                } else if !inst.is_legacy() && w.is_legacy() {
                    push("error", "Tagged Legacy (UE4) – UE4 paks don't load on Enhanced. Look for an Enhanced version.".into());
                } else if !inst.is_legacy() && !w.is_enhanced() && info.is_none() {
                    push("warn", "Not tagged Enhanced on the Workshop – check the mod page that it supports Enhanced.".into());
                }
                if w.banned {
                    push("error", "Banned on the Workshop.".into());
                }
            }
        }
        if let Some(i) = &info {
            if !inst.is_legacy() {
                if let Some(rev) = i.devkit_revision {
                    if rev != cx.server_devkit {
                        push(
                            "error",
                            format!("Built with devkit revision {rev}; this server build accepts {}. The author needs to re-cook it.", cx.server_devkit),
                        );
                    }
                }
            }
            if let Some(p) = &pak {
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if !i.folder_name.is_empty() && !stem.eq_ignore_ascii_case(&i.folder_name) && m.workshop_id.is_empty() {
                    push("warn", format!("Pak file was renamed (mod expects {}.pak) – renamed paks fail to load on Enhanced.", i.folder_name));
                }
            }
            let section = http_cfg.get(&i.folder_name.to_lowercase());
            let pending: Vec<&HttpOrigin> = i
                .requested_http_origins
                .iter()
                .filter(|o| section.map(|(a, d)| !a.contains(&o.origin) && !d.contains(&o.origin)).unwrap_or(true))
                .collect();
            if !pending.is_empty() {
                push("warn", format!("Requests internet access to {} – approve or deny it under HTTP access.", pending.iter().map(|o| o.origin.as_str()).collect::<Vec<_>>().join(", ")));
            }
        } else if pak.is_some() && !inst.is_legacy() {
            push("warn", "No modinfo.json found in the pak – it may be a Legacy (UE4) mod.".into());
        }
        if update_available {
            push("info", "An update is available on the Workshop.".into());
        }
        if m.enabled && !cx.loaded.is_empty() && !loaded && pak.is_some() {
            push("warn", "Enabled, but the running server didn't mount it this session (restart pending, or it failed to load – check the log).".into());
        }
        let errors = issues.iter().filter(|i| i.level == "error").count() as u32;
        let warns = issues.iter().filter(|i| i.level == "warn").count() as u32;
        let mut health = 100u32.saturating_sub(errors * 45 + warns * 15);
        if update_available {
            health = health.saturating_sub(5);
        }
        if let Some(w) = &ws {
            // Stale mods (no update for a year) are riskier after game patches.
            let age_days = (chrono::Utc::now().timestamp() - w.time_updated) / 86_400;
            if w.found && age_days > 365 {
                health = health.saturating_sub(10);
            }
        }
        let mut entry = m.clone();
        if entry.name.is_empty() {
            entry.name = ws.as_ref().map(|w| w.title.clone()).filter(|t| !t.is_empty()).or_else(|| info.as_ref().map(|i| i.name.clone())).unwrap_or_else(|| m.key());
        }
        mods.push(ModView { key: m.key(), entry, pak, size, info, workshop: ws, local_updated, update_available, loaded, issues, health });
    }

    // Conflicts: two or more *enabled* mods overriding the same data-table row this session.
    let conflicts = cx
        .row_overrides
        .iter()
        .filter(|(_, ms)| {
            let mut u: Vec<String> = ms.to_vec();
            u.dedup();
            u.len() > 1
        })
        .map(|(row, ms)| {
            let mut u: Vec<String> = ms.to_vec();
            u.dedup();
            ModConflict { row: row.clone(), mods: u }
        })
        .collect();

    // HTTP origin requests: from modinfo + those the server log reported.
    let mut http: Vec<HttpRequestView> = vec![];
    let mut add = |folder: &str, origin: &str, reason: &str| {
        if http.iter().any(|h| h.mod_folder.eq_ignore_ascii_case(folder) && h.origin == origin) {
            return;
        }
        let st = match http_cfg.get(&folder.to_lowercase()) {
            Some((a, _)) if a.iter().any(|x| x == origin) => "allowed",
            Some((_, d)) if d.iter().any(|x| x == origin) => "denied",
            _ => "pending",
        };
        http.push(HttpRequestView { mod_folder: folder.into(), origin: origin.into(), reason: reason.into(), state: st.into() });
    };
    for v in &mods {
        if let Some(i) = &v.info {
            for o in &i.requested_http_origins {
                add(&i.folder_name, &o.origin, &o.reason);
            }
        }
    }
    for (folder, origin, reason) in cx.log_http {
        add(folder, origin, reason);
    }
    // Approvals already in ModHttp.ini for mods we don't see requests from still show up.
    for (folder, (allowed, denied)) in &http_cfg {
        for o in allowed {
            add(folder, o, "");
        }
        for o in denied {
            add(folder, o, "");
        }
    }

    let mods_path = inst.project_dir().join("Content").join("Paks").join("~mods");
    ModsView { mods, server_devkit: cx.server_devkit, conflicts, http, workshop_dir: cx.cache.to_path_buf(), folder_exists: mods_path.is_dir(), mods_path }
}

// --------------------------------------------------------------------------------- ModHttp.ini

fn unquote(v: &str) -> String {
    let t = v.split(" ;").next().unwrap_or(v).trim();
    t.trim_matches('"').to_string()
}

/// folder (lowercase) -> (allowed origins, denied origins)
pub fn read_modhttp(inst: &ServerInstance) -> HashMap<String, (Vec<String>, Vec<String>)> {
    let text = ini::read_file(&inst.modhttp_ini()).unwrap_or_default();
    let doc = IniDoc::parse(&text);
    let mut out = HashMap::new();
    for s in doc.sections() {
        let mut a = vec![];
        let mut d = vec![];
        for (k, v) in doc.pairs(&s) {
            if k.eq_ignore_ascii_case("AllowedOrigin") {
                a.push(unquote(&v));
            } else if k.eq_ignore_ascii_case("DeniedOrigin") {
                d.push(unquote(&v));
            }
        }
        out.insert(s.to_lowercase(), (a, d));
    }
    out
}

/// Allow or deny one origin for one mod (lossless edit of ModHttp.ini; values are quoted because
/// an unquoted `//` would start a comment – per Funcom's docs).
pub fn set_http_origin(inst: &ServerInstance, folder: &str, origin: &str, allow: bool) -> AppResult<()> {
    let origin = origin.trim();
    if !(origin.starts_with("https://") || origin.starts_with("http://")) || origin.contains(' ') {
        return Err(AppError::msg("An origin looks like https://host[:port]"));
    }
    let path = inst.modhttp_ini();
    let mut doc = IniDoc::parse(&ini::read_file(&path)?);
    let section = doc.sections().into_iter().find(|s| s.eq_ignore_ascii_case(folder)).unwrap_or_else(|| folder.to_string());
    let quoted = format!("\"{origin}\"");
    doc.remove_value(&section, "AllowedOrigin", &quoted);
    doc.remove_value(&section, "DeniedOrigin", &quoted);
    doc.add(&section, if allow { "AllowedOrigin" } else { "DeniedOrigin" }, &quoted);
    ini::write_file(&path, &doc.to_text())
}

// ------------------------------------------------------------------------- version archive

/// Copy the current pak of a Workshop item aside before it's updated, so it can be rolled back.
/// Keeps the newest `keep` versions per item.
pub fn archive_current(cache: &Path, id: &str, keep: usize) -> Option<PathBuf> {
    let pak = workshop_pak(cache, id)?;
    let stamp = steamcmd::workshop_local_times(cache).get(id).copied().unwrap_or_else(|| chrono::Utc::now().timestamp());
    let dir = cache.join("_history").join(id).join(stamp.to_string());
    if dir.exists() {
        return Some(dir);
    }
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::copy(&pak, dir.join(pak.file_name()?)).ok()?;
    let root = cache.join("_history").join(id);
    if let Ok(rd) = std::fs::read_dir(&root) {
        let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        v.sort();
        if v.len() > keep {
            for old in &v[..v.len() - keep] {
                let _ = std::fs::remove_dir_all(old);
            }
        }
    }
    Some(dir)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedVersion {
    pub stamp: i64,
    pub path: PathBuf,
    pub size: u64,
}

pub fn archived_versions(cache: &Path, id: &str) -> Vec<ArchivedVersion> {
    let root = cache.join("_history").join(id);
    let mut v: Vec<ArchivedVersion> = std::fs::read_dir(&root)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let stamp = e.file_name().to_string_lossy().parse().ok()?;
                    let pak = std::fs::read_dir(e.path()).ok()?.flatten().map(|f| f.path()).find(|p| p.extension().map(|x| x == "pak").unwrap_or(false))?;
                    let size = std::fs::metadata(&pak).map(|m| m.len()).unwrap_or(0);
                    Some(ArchivedVersion { stamp, path: pak, size })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    v
}

/// Put an archived pak back in place of the current Workshop copy (the current one is archived
/// first). The next Workshop update replaces it again unless the mod is pinned by the user.
pub fn rollback(cache: &Path, id: &str, stamp: i64) -> AppResult<()> {
    let ver = archived_versions(cache, id).into_iter().find(|v| v.stamp == stamp).ok_or_else(|| AppError::msg("That archived version no longer exists."))?;
    archive_current(cache, id, 5);
    let dir = steamcmd::workshop_item_dir(cache, id);
    std::fs::create_dir_all(&dir)?;
    if let Some(cur) = workshop_pak(cache, id) {
        std::fs::remove_file(cur)?;
    }
    std::fs::copy(&ver.path, dir.join(ver.path.file_name().unwrap_or_default()))?;
    Ok(())
}

pub fn total_size(cache: &Path, inst: &ServerInstance) -> u64 {
    inst.mods.iter().filter_map(|m| resolve_pak(cache, m)).filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape of the real embedded manifest + modinfo.json (Growth Rates 100, captured from the pak).
    const REAL: &str = "\x00\x01junk{\r\n  \"files\": {\r\n    \"modinfo.json\": \"7213d8170f5aebb845b95c8e0d957c68\"\r\n  }\r\n}\x00\x00{\r\n\t\"name\": \"Growth Rates 100\",\r\n\t\"description\": \"Sets {growth} to 100% \\\"quoted\\\"\",\r\n\t\"changeNote\": \"v1.0.10\\r\\nRe-cook for 10/6/26 update\",\r\n\t\"author\": \"STuRoK\",\r\n\t\"authorUrl\": \"https://steamcommunity.com/profiles/76561197991373096/myworkshopfiles/?appid=440900\",\r\n\t\"versionMajor\": 1,\r\n\t\"versionMinor\": 0,\r\n\t\"versionBuild\": 10,\r\n\t\"bRequiresLoadOnStartup\": false,\r\n\t\"steamWorkshopFileIds\": {\r\n\t\t\"mainClient\": \"3745387835\"\r\n\t},\r\n\t\"folderName\": \"Growth_Rates_100\",\r\n\t\"devkitRevisionNumber\": 1002,\r\n\t\"minimumVersion\": \"Enhanced\",\r\n\t\"requestedHttpOrigins\": [\"https://api.example.com\"]\r\n}\x00\x00more";

    #[test]
    fn modinfo_found_in_binary() {
        let i = find_modinfo(REAL.as_bytes()).expect("modinfo");
        assert_eq!(i.name, "Growth Rates 100");
        assert_eq!(i.folder_name, "Growth_Rates_100");
        assert_eq!(i.devkit_revision, Some(1002));
        assert_eq!(i.version, "1.0.10");
        assert_eq!(i.workshop_id, "3745387835");
        assert_eq!(i.minimum_version, "Enhanced");
        assert_eq!(i.requested_http_origins[0].origin, "https://api.example.com");
    }

    #[test]
    fn modinfo_from_real_pak_if_present() {
        // Uses the verification download when it exists on this machine (git-ignored).
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.test-workshop/steamapps/workshop/content/440900/3815570855/Exiles_VoiceRange.pak");
        if p.exists() {
            let i = read_modinfo(&p).expect("modinfo in real pak");
            assert_eq!(i.folder_name, "Exiles_VoiceRange");
            assert_eq!(i.devkit_revision, Some(1002));
        }
    }

    #[test]
    fn modlist_parsing() {
        let md = Path::new(r"D:\srv\ConanSandbox\Mods");
        let t = "O:\\cache\\steamapps\\workshop\\content\\440900\\3745387835\\Growth_Rates_100.pak\r\n*Pippi.pak\r\n\r\nD:\\paks\\Local.pak\r\n";
        let v = parse_modlist(t, md);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].workshop_id, "3745387835");
        assert_eq!(v[1].local_path.as_deref(), Some(md.join("Pippi.pak").as_path()));
        assert_eq!(v[2].local_path.as_deref(), Some(Path::new(r"D:\paks\Local.pak")));
    }

    #[test]
    fn modhttp_round_trip() {
        let root = std::env::temp_dir().join(format!("cesm-mh-{}", uuid::Uuid::new_v4()));
        let inst = ServerInstance { install_dir: root.clone(), ..Default::default() };
        std::fs::create_dir_all(inst.config_dir()).unwrap();
        set_http_origin(&inst, "HttpTest", "https://discord.com:443", true).unwrap();
        set_http_origin(&inst, "HttpTest", "https://httpbin.org", false).unwrap();
        set_http_origin(&inst, "HttpTest", "https://discord.com:443", true).unwrap();
        let text = std::fs::read_to_string(inst.modhttp_ini()).unwrap();
        assert_eq!(text.matches("AllowedOrigin=\"https://discord.com:443\"").count(), 1);
        assert!(text.contains("DeniedOrigin=\"https://httpbin.org\""));
        let m = read_modhttp(&inst);
        assert_eq!(m["httptest"].0, vec!["https://discord.com:443"]);
        set_http_origin(&inst, "HttpTest", "https://httpbin.org", true).unwrap();
        assert!(read_modhttp(&inst)["httptest"].1.is_empty());
        assert!(set_http_origin(&inst, "X", "ftp://nope", true).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn server_mod_list_order() {
        let mut inst = ServerInstance::default();
        inst.mods = vec![
            ModEntry { workshop_id: "2".into(), ..Default::default() },
            ModEntry { workshop_id: "1".into(), enabled: false, ..Default::default() },
            ModEntry { workshop_id: "3".into(), ..Default::default() },
        ];
        assert_eq!(server_mod_list(&inst), "2,3");
    }
}
