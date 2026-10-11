//! Async mod orchestration: Workshop metadata (cached in manager.db), update detection, downloads
//! through the shared SteamCMD cache, and archiving the previous version before an update.

use super::workshop::{self, WorkshopItem};
use super::{mods, steamcmd};
use crate::error::AppResult;
use crate::models::*;
use crate::state::{now_ms, AppState, TaskReporter};
use serde::Serialize;
use std::collections::HashMap;
use tauri::AppHandle;

/// Display metadata for these ids: cached (24 h) unless `fresh`.
pub async fn workshop_info(state: &AppState, ids: &[String], fresh: bool) -> HashMap<String, WorkshopItem> {
    let mut out = HashMap::new();
    let mut need = vec![];
    for id in ids {
        let cached = if fresh { None } else { state.history.cached_workshop(id, 24 * 3_600_000) };
        match cached.and_then(|j| serde_json::from_str::<WorkshopItem>(&j).ok()) {
            Some(w) => {
                out.insert(id.clone(), w);
            }
            None => need.push(id.clone()),
        }
    }
    if !need.is_empty() {
        // Retry once on transient failure (Steam API can be flaky).
        let result = match workshop::details(&state.http, &need).await {
            Ok(items) => Ok(items),
            Err(e1) => {
                tracing::warn!("Workshop details failed (retrying): {e1}");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                workshop::details(&state.http, &need).await
            }
        };
        match result {
            Ok(items) => {
                for w in items {
                    if let Ok(j) = serde_json::to_string(&w) {
                        state.history.cache_workshop(&w.id, &j);
                    }
                    out.insert(w.id.clone(), w);
                }
            }
            Err(e) => {
                tracing::warn!("Workshop details failed after retry: {e}");
                // Fall back to stale cache rather than showing nothing.
                for id in need {
                    if let Some(w) = state.history.cached_workshop(&id, i64::MAX / 2).and_then(|j| serde_json::from_str(&j).ok()) {
                        out.insert(id, w);
                    }
                }
            }
        }
    }
    out
}

/// Workshop ids of enabled mods whose Workshop copy is newer than the one on disk.
pub async fn outdated(state: &AppState, inst: &ServerInstance) -> Vec<String> {
    let ids: Vec<String> = inst.mods.iter().filter(|m| !m.workshop_id.is_empty()).map(|m| m.workshop_id.clone()).collect();
    if ids.is_empty() {
        return vec![];
    }
    let info = workshop_info(state, &ids, true).await;
    let cache = state.workshop_dir().await;
    let local = steamcmd::workshop_local_times(&cache);
    ids.into_iter()
        .filter(|id| match (info.get(id), local.get(id)) {
            (Some(w), Some(l)) => w.found && w.time_updated > *l,
            _ => false,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub downloaded: Vec<String>,
    pub updated: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Make sure every enabled Workshop mod is on disk; with `update`, also fetch newer versions
/// (the current pak is archived first so it can be rolled back).
pub async fn sync(state: &AppState, app: &AppHandle, inst: &ServerInstance, update: bool, quiet_when_nothing: bool) -> AppResult<SyncReport> {
    let cache = state.workshop_dir().await;
    let enabled: Vec<String> = inst.mods.iter().filter(|m| m.enabled && !m.workshop_id.is_empty()).map(|m| m.workshop_id.clone()).collect();
    let missing: Vec<String> = enabled.iter().filter(|id| mods::workshop_pak(&cache, id).is_none()).cloned().collect();
    let stale: Vec<String> = if update { outdated(state, inst).await.into_iter().filter(|id| enabled.contains(id) && !missing.contains(id)).collect() } else { vec![] };
    let mut report = SyncReport::default();
    if missing.is_empty() && stale.is_empty() {
        if !quiet_when_nothing {
            TaskReporter::new(app, "mods", "Mods", Some(&inst.id)).done("All mods are up to date");
        }
        return Ok(report);
    }
    for id in &stale {
        mods::archive_current(&cache, id, 5);
    }
    let mut ids = missing.clone();
    ids.extend(stale.iter().cloned());
    let r = TaskReporter::new(app, "steamcmd", &format!("Downloading mods for {}", inst.name), Some(&inst.id));
    let cancel = state.register_cancel(&r.task_id).await;
    let steam_dir = state.steamcmd_dir().await;
    let res = {
        let _l = state.steam_lock.lock().await;
        steamcmd::workshop_download(&state.http, &steam_dir, &cache, &ids, false, &r, Some(cancel)).await
    };
    state.cancels.lock().await.remove(&r.task_id);
    let res = match res {
        Ok(x) => x,
        Err(e) => {
            r.fail(&e.to_string());
            return Err(e);
        }
    };
    report.downloaded = res.downloaded.iter().filter(|i| missing.contains(i)).cloned().collect();
    report.updated = res.downloaded.iter().filter(|i| stale.contains(i)).cloned().collect();
    report.failed = res.failed.clone();
    state.with_runtime(&inst.id, |rt| rt.mod_updates.retain(|i| !report.updated.contains(i))).await;
    if report.failed.is_empty() {
        r.done(&format!("{} downloaded, {} updated", report.downloaded.len(), report.updated.len()));
    } else {
        r.fail(&format!(
            "{} mod(s) failed: {}",
            report.failed.len(),
            report.failed.iter().map(|(i, w)| format!("{i} ({w})")).collect::<Vec<_>>().join(", ")
        ));
    }
    if !report.updated.is_empty() {
        state.history.event(Some(&inst.id), "info", "mods", &format!("Updated mods: {}", report.updated.join(", ")));
    }
    let _ = now_ms();
    Ok(report)
}
