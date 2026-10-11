//! Game rules: `ServerSettings.ini [ServerSettings]`.
//!
//! The file is the source of truth – the server writes it itself (on `SetServerSetting` and on
//! settings upgrades), so nothing here is cached in the profile. Write path (docs/DESIGN.md §5.2):
//! stopped → lossless file edit; running + RCON → `SetServerSetting` (the server persists it);
//! running without RCON → queued and written once the server has exited.

use super::ini::{self, IniDoc};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::AppState;
use serde::Serialize;
use std::collections::BTreeMap;

/// Keys the profile owns (Server Setup) or the server owns – never edited through game rules.
pub const PROTECTED: &[&str] = &["AdminPassword", "ServerPassword", "ServerMessageOfTheDay", "ServerModList", "ConfigVersion", "BlueprintConfigVersion"];

pub fn is_protected(key: &str) -> bool {
    PROTECTED.iter().any(|p| p.eq_ignore_ascii_case(key))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRules {
    pub exists: bool,
    /// Every key in `[ServerSettings]` (protected ones included, read-only in the UI).
    pub values: BTreeMap<String, String>,
    /// Changes waiting for the server to stop (no RCON while running).
    pub pending: BTreeMap<String, String>,
    pub config_version: Option<String>,
    pub blueprint_config_version: Option<String>,
}

pub fn read(inst: &ServerInstance) -> AppResult<GameRules> {
    let path = inst.settings_path();
    let text = ini::read_file(&path)?;
    let doc = IniDoc::parse(&text);
    let values: BTreeMap<String, String> = doc.pairs(SETTINGS_SECTION).into_iter().collect();
    Ok(GameRules {
        exists: path.exists(),
        config_version: values.get("ConfigVersion").cloned(),
        blueprint_config_version: values.get("BlueprintConfigVersion").cloned(),
        values,
        pending: BTreeMap::new(),
    })
}

fn check_value(key: &str, value: &str) -> AppResult<()> {
    if key.trim().is_empty() || key.contains(['=', '[', ']', '\r', '\n', ' ']) {
        return Err(AppError::msg(format!("“{key}” isn't a valid setting name.")));
    }
    if value.contains(['\r', '\n']) {
        return Err(AppError::msg(format!("{key}: values can't contain line breaks.")));
    }
    Ok(())
}

/// Write values straight into the file (server stopped). Lossless: only these keys change.
pub fn write_file_values(inst: &ServerInstance, values: &BTreeMap<String, String>) -> AppResult<()> {
    let path = inst.settings_path();
    let mut doc = IniDoc::parse(&ini::read_file(&path)?);
    for (k, v) in values {
        check_value(k, v)?;
        doc.set(SETTINGS_SECTION, k, v);
    }
    ini::write_file(&path, &doc.to_text())
}

/// Remove keys (reset to the game's built-in default).
pub fn remove_file_values(inst: &ServerInstance, keys: &[String]) -> AppResult<()> {
    let path = inst.settings_path();
    let mut doc = IniDoc::parse(&ini::read_file(&path)?);
    for k in keys {
        doc.remove(SETTINGS_SECTION, k);
    }
    ini::write_file(&path, &doc.to_text())
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    /// Applied to the running server over RCON (and persisted by it).
    pub live: Vec<String>,
    /// Written to the file (server stopped).
    pub written: Vec<String>,
    /// Waiting for the server to stop.
    pub queued: Vec<String>,
    pub failed: Vec<(String, String)>,
    /// Some applied keys only take effect after a restart.
    pub restart_needed: bool,
}

/// Apply game-rule changes the best way for the server's current state.
/// `live_values` overrides what is sent over RCON for a key: enum settings are stored as numbers
/// in the file but `SetServerSetting` only accepts their names (verified: `SetServerSetting
/// MaxNudity 2` is ignored, `SetServerSetting MaxNudity Full` writes `MaxNudity=2`).
pub async fn apply(
    state: &AppState,
    inst: &ServerInstance,
    changes: BTreeMap<String, String>,
    live_values: &BTreeMap<String, String>,
    restart_keys: &[String],
) -> AppResult<ApplyResult> {
    let mut res = ApplyResult::default();
    let mut ok: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in changes {
        if is_protected(&k) {
            res.failed.push((k, "managed under Server Setup".into()));
            continue;
        }
        match check_value(&k, &v) {
            Ok(()) => {
                ok.insert(k, v);
            }
            Err(e) => res.failed.push((k, e.to_string())),
        }
    }
    let (_, main) = super::server::running_pids(inst);
    if main.is_none() {
        write_file_values(inst, &ok)?;
        res.written = ok.keys().cloned().collect();
        return Ok(res);
    }
    for (i, (k, v)) in ok.iter().enumerate() {
        if i > 0 {
            // Stay well under the RCON rate limit on big preset applies.
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let sent = live_values.get(k).unwrap_or(v);
        // RCON arguments are space separated; the value is the remainder of the line.
        match state.rcon_exec(inst, &format!("SetServerSetting {k} {sent}")).await {
            Ok(reply) if reply.to_lowercase().starts_with("setting") => res.live.push(k.clone()),
            Ok(reply) => res.failed.push((k.clone(), reply)),
            Err(_) => {
                state.with_runtime(&inst.id, |r| {
                    r.pending_settings.insert(k.clone(), v.clone());
                    r.pending_restart = true;
                })
                .await;
                res.queued.push(k.clone());
            }
        }
    }
    res.restart_needed = !res.queued.is_empty() || res.live.iter().any(|k| restart_keys.iter().any(|r| r.eq_ignore_ascii_case(k)));
    if res.restart_needed {
        state.with_runtime(&inst.id, |r| r.pending_restart = true).await;
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_values_lossless_and_validated() {
        let dir = std::env::temp_dir().join(format!("cesm-set-{}", uuid::Uuid::new_v4()));
        let inst = ServerInstance { install_dir: dir.clone(), ..Default::default() };
        std::fs::create_dir_all(inst.config_dir()).unwrap();
        std::fs::write(inst.settings_path(), ";METADATA=(Diff=true, UseCommands=true)\r\n[ServerSettings]\r\nConfigVersion=13\r\nPVPEnabled=False\r\n").unwrap();
        let mut m = BTreeMap::new();
        m.insert("PVPEnabled".to_string(), "True".to_string());
        m.insert("HarvestAmountMultiplier".to_string(), "3".to_string());
        write_file_values(&inst, &m).unwrap();
        let r = read(&inst).unwrap();
        assert_eq!(r.values["PVPEnabled"], "True");
        assert_eq!(r.values["HarvestAmountMultiplier"], "3");
        assert_eq!(r.config_version.as_deref(), Some("13"));
        let t = std::fs::read_to_string(inst.settings_path()).unwrap();
        assert!(t.starts_with(";METADATA=(Diff=true, UseCommands=true)\r\n[ServerSettings]\r\nConfigVersion=13\r\nPVPEnabled=True\r\n"));
        let mut bad = BTreeMap::new();
        bad.insert("Evil".to_string(), "1\r\n[Other]".to_string());
        assert!(write_file_values(&inst, &bad).is_err());
        remove_file_values(&inst, &["HarvestAmountMultiplier".into()]).unwrap();
        assert!(!read(&inst).unwrap().values.contains_key("HarvestAmountMultiplier"));
        assert!(is_protected("adminpassword"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
