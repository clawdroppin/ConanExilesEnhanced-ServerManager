//! Ban list and whitelist: `ConanSandbox/Saved/blacklist.txt` and `Saved/whitelist.txt`.
//!
//! Verified against the Enhanced server (CL-379176): `whitelistplayer <id>` appends
//! `<id>:Unknown` (CRLF) to whitelist.txt, `unwhitelistplayer <id>` removes the line. The ban list
//! uses the same `<id>[:<name>]` line format. `banplayer` over RCON only accepts players the server
//! knows (`banplayer (index|name|userid|platformid|player) <id> <message>`); unknown ids reply
//! "No player with …". While the server runs it owns both files (it rewrites them on changes), so
//! edits then go over RCON; when it is stopped the manager edits the files directly.
//! Whitelisting only takes effect with `EnableWhitelist=True` in ServerSettings.ini.

use super::ini;
use crate::error::{AppError, AppResult};
use crate::models::ServerInstance;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListEntry {
    /// "ban" | "whitelist"
    pub list: String,
    pub id: String,
    pub name: String,
}

pub fn list_path(inst: &ServerInstance, list: &str) -> AppResult<PathBuf> {
    match list {
        "ban" => Ok(inst.saved_dir().join("blacklist.txt")),
        "whitelist" => Ok(inst.saved_dir().join("whitelist.txt")),
        _ => Err(AppError::msg("Unknown list")),
    }
}

pub fn parse(list: &str, text: &str) -> Vec<ListEntry> {
    text.lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}'))
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
        .map(|l| {
            let (id, name) = l.split_once(':').unwrap_or((l, ""));
            let name = name.trim();
            ListEntry {
                list: list.into(),
                id: id.trim().to_string(),
                name: if name.eq_ignore_ascii_case("unknown") { String::new() } else { name.to_string() },
            }
        })
        .collect()
}

pub fn read(inst: &ServerInstance) -> AppResult<Vec<ListEntry>> {
    let mut out = vec![];
    for list in ["ban", "whitelist"] {
        out.extend(parse(list, &ini::read_file(&list_path(inst, list)?)?));
    }
    Ok(out)
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Add or remove an id in the file (server stopped). Returns whether anything changed.
pub fn edit_file(inst: &ServerInstance, list: &str, add: bool, id: &str, name: &str) -> AppResult<bool> {
    if !valid_id(id) {
        return Err(AppError::msg("Player ids are letters and digits only (a Steam/platform id or a Funcom user id)."));
    }
    let path = list_path(inst, list)?;
    let text = ini::read_file(&path)?;
    let mut lines: Vec<String> = text.lines().map(|l| l.trim_end().to_string()).filter(|l| !l.trim().is_empty()).collect();
    let before = lines.len();
    let matches = |l: &str| l.split(':').next().unwrap_or("").trim().trim_start_matches('\u{feff}').eq_ignore_ascii_case(id);
    if add {
        if lines.iter().any(|l| matches(l)) {
            return Ok(false);
        }
        let name = name.replace([':', '\r', '\n'], " ");
        lines.push(format!("{id}:{}", if name.trim().is_empty() { "Unknown" } else { name.trim() }));
    } else {
        lines.retain(|l| !matches(l));
        if lines.len() == before {
            return Ok(false);
        }
    }
    std::fs::create_dir_all(inst.saved_dir())?;
    let mut out = lines.join("\r\n");
    if !out.is_empty() {
        out.push_str("\r\n");
    }
    std::fs::write(&path, out)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_files_round_trip() {
        let dir = std::env::temp_dir().join(format!("cesm-acc-{}", uuid::Uuid::new_v4()));
        let inst = ServerInstance { install_dir: dir.clone(), ..Default::default() };
        assert!(edit_file(&inst, "whitelist", true, "76561190000000002", "").unwrap());
        assert!(!edit_file(&inst, "whitelist", true, "76561190000000002", "dup").unwrap());
        assert!(edit_file(&inst, "ban", true, "ABCDEF0123", "Griefer").unwrap());
        let t = std::fs::read_to_string(list_path(&inst, "whitelist").unwrap()).unwrap();
        assert_eq!(t, "76561190000000002:Unknown\r\n");
        let all = read(&inst).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], ListEntry { list: "ban".into(), id: "ABCDEF0123".into(), name: "Griefer".into() });
        assert_eq!(all[1].name, "");
        assert!(edit_file(&inst, "ban", false, "abcdef0123", "").unwrap());
        assert!(read(&inst).unwrap().iter().all(|e| e.list == "whitelist"));
        assert!(edit_file(&inst, "ban", true, "bad id\r\n", "").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
