//! Backups, restore, retention and portable server packs (`.cesmpack`).
//!
//! A backup is a zip:
//! ```text
//! backup.json            manifest: label, kind, build, files + SHA-256, integrity result
//! World/game_0.db        hot snapshot via SQLite's online-backup API (safe while running)
//! Saved/serveruid.txt
//! Config/*.ini           ServerSettings, Engine, Game, ModHttp (no _history)
//! Mods/modlist.json      the profile's mod list + load order
//! Mods/paks/*.pak        optional
//! ```
//! Restore quarantines what's there first and removes stale `-wal`/`-shm` files – a restored DB
//! next to an old WAL would be corrupted on the next open.

use super::{gamedb, mods};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{now_ms, AppState};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

const MANIFEST: &str = "backup.json";
const CONFIG_FILES: &[&str] = &["ServerSettings.ini", "DedicatedServer.ini", "Engine.ini", "Game.ini", "ModHttp.ini"];

fn opts() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .large_file(true)
}

fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Manifest {
    format: u32,
    label: String,
    kind: String,
    created_at: i64,
    server_name: String,
    files: u32,
    verified: bool,
    includes_mods: bool,
    build: Option<String>,
    app_version: String,
    /// zip entry -> sha256
    checksums: BTreeMap<String, String>,
}

struct Writer<W: Write + std::io::Seek> {
    zip: zip::ZipWriter<W>,
    sums: BTreeMap<String, String>,
}

impl<W: Write + std::io::Seek> Writer<W> {
    fn add_bytes(&mut self, name: &str, bytes: &[u8]) -> AppResult<()> {
        self.zip.start_file(name, opts())?;
        self.zip.write_all(bytes)?;
        self.sums.insert(name.to_string(), sha256(bytes));
        Ok(())
    }
    /// Files are read fully first so a file being written at that moment is never half-streamed.
    fn add_file(&mut self, name: &str, path: &Path) -> AppResult<bool> {
        let mut buf = vec![];
        if std::fs::File::open(path).and_then(|mut f| f.read_to_end(&mut buf)).is_err() {
            return Ok(false); // locked – skip rather than fail the whole backup
        }
        self.add_bytes(name, &buf)?;
        Ok(true)
    }
}

pub async fn create(state: &AppState, inst: &ServerInstance, label: &str, kind: &str) -> AppResult<BackupInfo> {
    let dir = state.backup_dir(inst).await;
    let cache = state.workshop_dir().await;
    let build = state.with_runtime(&inst.id, |r| r.installed_build.clone()).await;
    let (inst, label, kind) = (inst.clone(), label.to_string(), kind.to_string());
    let tmp_dir = state.data_dir.join("tmp");
    tokio::task::spawn_blocking(move || {
        let info = create_blocking(&inst, &dir, &cache, &tmp_dir, &label, &kind, build)?;
        prune(&dir, inst.automation.backup_keep);
        Ok(info)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

pub fn create_blocking(
    inst: &ServerInstance,
    dir: &Path,
    cache: &Path,
    tmp_dir: &Path,
    label: &str,
    kind: &str,
    build: Option<String>,
) -> AppResult<BackupInfo> {
    std::fs::create_dir_all(dir)?;
    std::fs::create_dir_all(tmp_dir)?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let path = dir.join(format!("{stamp}_{kind}.zip"));
    let part = path.with_extension("zip.part");
    let mut w = Writer { zip: zip::ZipWriter::new(std::fs::File::create(&part)?), sums: BTreeMap::new() };
    let mut verified = false;

    let db = inst.db_path();
    if db.exists() {
        let snap = tmp_dir.join(format!("snap-{}.db", uuid::Uuid::new_v4()));
        let res = gamedb::snapshot(&db, &snap);
        match res {
            Ok(ok) => {
                verified = ok;
                w.add_file(&format!("World/{}", db.file_name().unwrap_or_default().to_string_lossy()), &snap)?;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&snap);
                let _ = std::fs::remove_file(&part);
                return Err(AppError::msg(format!("Couldn't snapshot the world database: {e}")));
            }
        }
        let _ = std::fs::remove_file(&snap);
    }
    let uid = inst.saved_dir().join("serveruid.txt");
    if uid.exists() {
        w.add_file("Saved/serveruid.txt", &uid)?;
    }
    for f in CONFIG_FILES {
        let p = inst.config_dir().join(f);
        if p.exists() {
            w.add_file(&format!("Config/{f}"), &p)?;
        }
    }
    let list: Vec<ModEntry> = inst.mods.clone();
    w.add_bytes("Mods/modlist.json", &serde_json::to_vec_pretty(&list)?)?;
    let include_mods = inst.automation.backup_include_mods || kind == "full";
    if include_mods {
        for m in inst.mods.iter().filter(|m| m.enabled) {
            if let Some(p) = mods::resolve_pak(cache, m) {
                let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let folder = if m.workshop_id.is_empty() { "local".to_string() } else { m.workshop_id.clone() };
                w.add_file(&format!("Mods/paks/{folder}/{name}"), &p)?;
            }
        }
    }
    let files = w.sums.len() as u32;
    let created_at = now_ms();
    let m = Manifest {
        format: 2,
        label: label.to_string(),
        kind: kind.to_string(),
        created_at,
        server_name: inst.name.clone(),
        files,
        verified,
        includes_mods: include_mods,
        build: build.clone(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        checksums: w.sums.clone(),
    };
    w.zip.start_file(MANIFEST, opts())?;
    w.zip.write_all(&serde_json::to_vec_pretty(&m)?)?;
    w.zip.finish()?;
    std::fs::rename(&part, &path)?;
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Ok(BackupInfo {
        name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        path,
        size,
        created_at,
        label: label.to_string(),
        kind: kind.to_string(),
        files,
        verified,
        includes_mods: include_mods,
        build,
    })
}

fn read_manifest(path: &Path) -> Option<Manifest> {
    let f = std::fs::File::open(path).ok()?;
    let mut z = zip::ZipArchive::new(f).ok()?;
    let mut e = z.by_name(MANIFEST).ok()?;
    let mut s = String::new();
    e.read_to_string(&mut s).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn list(dir: &Path) -> Vec<BackupInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<BackupInfo> = rd
        .flatten()
        .filter(|e| e.path().extension().map(|x| x.eq_ignore_ascii_case("zip")).unwrap_or(false))
        .map(|e| {
            let path = e.path();
            let meta = e.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta.and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(0);
            let m = read_manifest(&path);
            BackupInfo {
                name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                size,
                created_at: m.as_ref().map(|m| m.created_at).unwrap_or(mtime),
                label: m.as_ref().map(|m| m.label.clone()).unwrap_or_else(|| "Imported".into()),
                kind: m.as_ref().map(|m| m.kind.clone()).unwrap_or_else(|| "manual".into()),
                files: m.as_ref().map(|m| m.files).unwrap_or(0),
                verified: m.as_ref().map(|m| m.verified).unwrap_or(false),
                includes_mods: m.as_ref().map(|m| m.includes_mods).unwrap_or(false),
                build: m.as_ref().and_then(|m| m.build.clone()),
                path,
            }
        })
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Keep the newest `keep` automatic backups; manual (and "full") ones are never pruned.
pub fn prune(dir: &Path, keep: u32) -> u32 {
    if keep == 0 {
        return 0;
    }
    let autos: Vec<BackupInfo> = list(dir).into_iter().filter(|b| b.kind == "auto").collect();
    let mut removed = 0;
    for b in autos.iter().skip(keep as usize) {
        if std::fs::remove_file(&b.path).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub ok: bool,
    pub checked: u32,
    pub mismatched: Vec<String>,
    pub missing: Vec<String>,
    pub db_ok: Option<bool>,
}

/// Re-hash every entry against the manifest and integrity-check the world database inside.
pub fn verify(path: &Path, tmp_dir: &Path) -> AppResult<VerifyReport> {
    let m = read_manifest(path).ok_or_else(|| AppError::msg("This backup has no manifest (made by another tool?)."))?;
    let f = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(f)?;
    let mut report = VerifyReport { ok: true, checked: 0, mismatched: vec![], missing: vec![], db_ok: None };
    for (name, sum) in &m.checksums {
        match z.by_name(name) {
            Ok(mut e) => {
                let mut buf = vec![];
                e.read_to_end(&mut buf)?;
                report.checked += 1;
                if &sha256(&buf) != sum {
                    report.mismatched.push(name.clone());
                }
                if name.starts_with("World/") && name.ends_with(".db") {
                    std::fs::create_dir_all(tmp_dir)?;
                    let tmp = tmp_dir.join(format!("verify-{}.db", uuid::Uuid::new_v4()));
                    std::fs::write(&tmp, &buf)?;
                    let ok = rusqlite::Connection::open(&tmp)
                        .and_then(|c| c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)))
                        .map(|s| s == "ok")
                        .unwrap_or(false);
                    let _ = std::fs::remove_file(&tmp);
                    report.db_ok = Some(ok);
                }
            }
            Err(_) => report.missing.push(name.clone()),
        }
    }
    report.ok = report.mismatched.is_empty() && report.missing.is_empty() && report.db_ok != Some(false);
    Ok(report)
}

/// Move individual files aside into `Saved/_quarantine/<stamp>-<label>/`.
pub fn quarantine_files(inst: &ServerInstance, files: &[PathBuf], label: &str) -> AppResult<Option<PathBuf>> {
    let existing: Vec<&PathBuf> = files.iter().filter(|f| f.exists()).collect();
    if existing.is_empty() {
        return Ok(None);
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = inst.saved_dir().join("_quarantine").join(format!("{stamp}-{label}"));
    std::fs::create_dir_all(&dest)?;
    for f in existing {
        std::fs::rename(f, dest.join(f.file_name().unwrap_or_default()))?;
    }
    Ok(Some(dest))
}

fn extract_prefix(zip_path: &Path, prefix: &str, dest: &Path) -> AppResult<u32> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f)?;
    let mut n = 0;
    for i in 0..z.len() {
        let mut e = z.by_index(i)?;
        let Some(name) = e.enclosed_name() else { continue };
        let name_s = name.to_string_lossy().replace('\\', "/");
        let Some(rel) = name_s.strip_prefix(&format!("{prefix}/")) else { continue };
        if rel.is_empty() || e.is_dir() {
            continue;
        }
        let out = dest.join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut w = std::fs::File::create(&out)?;
        std::io::copy(&mut e, &mut w)?;
        n += 1;
    }
    Ok(n)
}

fn read_entry(zip_path: &Path, name: &str) -> Option<Vec<u8>> {
    let f = std::fs::File::open(zip_path).ok()?;
    let mut z = zip::ZipArchive::new(f).ok()?;
    let mut e = z.by_name(name).ok()?;
    let mut buf = vec![];
    e.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// The profile's mod list stored in a backup or pack.
pub fn backup_mod_list(zip_path: &Path) -> Option<Vec<ModEntry>> {
    serde_json::from_slice(&read_entry(zip_path, "Mods/modlist.json")?).ok()
}

/// Restore into a stopped server. The current world (and its WAL/SHM side files) is quarantined
/// first. Returns the number of files written.
pub fn restore(inst: &ServerInstance, zip_path: &Path, include_config: bool) -> AppResult<u32> {
    let saved = inst.saved_dir();
    std::fs::create_dir_all(&saved)?;
    let f = std::fs::File::open(zip_path)?;
    let z = zip::ZipArchive::new(f)?;
    let world: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|s| s.to_string())).filter(|n| n.starts_with("World/") && n.ends_with(".db")).collect();
    drop(z);
    let mut n = 0;
    if !world.is_empty() {
        let mut aside = vec![];
        for w in &world {
            let file = saved.join(w.trim_start_matches("World/"));
            aside.push(file.clone());
            aside.push(PathBuf::from(format!("{}-wal", file.display())));
            aside.push(PathBuf::from(format!("{}-shm", file.display())));
        }
        quarantine_files(inst, &aside, "before-restore")?;
        n += extract_prefix(zip_path, "World", &saved)?;
    }
    if let Some(uid) = read_entry(zip_path, "Saved/serveruid.txt") {
        std::fs::write(saved.join("serveruid.txt"), uid)?;
        n += 1;
    }
    if include_config {
        let cfg = inst.config_dir();
        std::fs::create_dir_all(&cfg)?;
        for f in CONFIG_FILES {
            if let Some(bytes) = read_entry(zip_path, &format!("Config/{f}")) {
                super::ini::write_file(&cfg.join(f), &String::from_utf8_lossy(&bytes))?;
                n += 1;
            }
        }
    }
    Ok(n)
}

// ---------------------------------------------------------------------------------------------
// Server packs (.cesmpack): manifest.json + Config/ + optional World/ + Mods/modlist.json

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackOptions {
    pub include_saves: bool,
    pub include_secrets: bool,
}

pub fn export_pack(inst: &ServerInstance, dest: &Path, o: &PackOptions, tmp_dir: &Path) -> AppResult<()> {
    let mut profile = inst.clone();
    if !o.include_secrets {
        profile.identity.admin_password.clear();
        profile.identity.server_password.clear();
        profile.rcon.password.clear();
        profile.branch_password.clear();
    }
    let part = dest.with_extension("part");
    let mut w = Writer { zip: zip::ZipWriter::new(std::fs::File::create(&part)?), sums: BTreeMap::new() };
    let mut has_configs = false;
    for f in CONFIG_FILES {
        let p = inst.config_dir().join(f);
        if p.exists() {
            let mut text = super::ini::read_file(&p)?;
            if !o.include_secrets {
                text = strip_secrets(f, &text);
            }
            w.add_bytes(&format!("Config/{f}"), text.as_bytes())?;
            has_configs = true;
        }
    }
    let mut has_saves = false;
    if o.include_saves && inst.db_path().exists() {
        std::fs::create_dir_all(tmp_dir)?;
        let snap = tmp_dir.join(format!("pack-{}.db", uuid::Uuid::new_v4()));
        gamedb::snapshot(&inst.db_path(), &snap)?;
        w.add_file("World/game_0.db", &snap)?;
        let _ = std::fs::remove_file(&snap);
        has_saves = true;
    }
    w.add_bytes("Mods/modlist.json", &serde_json::to_vec_pretty(&inst.mods)?)?;
    let m = PackManifest {
        format: 1,
        app_version: env!("CARGO_PKG_VERSION").into(),
        exported_at: now_ms(),
        instance: profile,
        has_saves,
        has_configs,
        includes_secrets: o.include_secrets,
    };
    w.zip.start_file("manifest.json", opts())?;
    w.zip.write_all(&serde_json::to_vec_pretty(&m)?)?;
    w.zip.finish()?;
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

/// Blank the password keys of a config file for sharing.
fn strip_secrets(file: &str, text: &str) -> String {
    let mut doc = super::ini::IniDoc::parse(text);
    match file {
        "ServerSettings.ini" => {
            for k in ["AdminPassword", "ServerPassword"] {
                if doc.get(SETTINGS_SECTION, k).is_some() {
                    doc.set(SETTINGS_SECTION, k, "");
                }
            }
        }
        "Engine.ini" => {
            if doc.get("OnlineSubsystem", "ServerPassword").is_some() {
                doc.set("OnlineSubsystem", "ServerPassword", "");
            }
        }
        "Game.ini" => {
            if doc.get("RconPlugin", "RconPassword").is_some() {
                doc.set("RconPlugin", "RconPassword", "");
            }
        }
        "DedicatedServer.ini" => {
            let section = "/Script/Dominion.DedicatedServerSettings";
            for k in ["Owner", "Whitelist", "Blacklist", "AdminList"] {
                if doc.get(section, k).is_some() {
                    doc.set(section, k, "");
                }
            }
        }
        _ => {}
    }
    doc.to_text()
}

pub fn read_pack(path: &Path) -> AppResult<PackManifest> {
    let bytes = read_entry(path, "manifest.json").ok_or_else(|| AppError::msg("That file isn't a Conan Exiles Server Manager pack (no manifest.json)."))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Unpack configs (and the world) of a pack into a (new) server's install folder.
pub fn unpack_into(path: &Path, inst: &ServerInstance) -> AppResult<()> {
    let cfg = inst.config_dir();
    std::fs::create_dir_all(&cfg)?;
    for f in CONFIG_FILES {
        if let Some(bytes) = read_entry(path, &format!("Config/{f}")) {
            super::ini::write_file(&cfg.join(f), &String::from_utf8_lossy(&bytes))?;
        }
    }
    std::fs::create_dir_all(inst.saved_dir())?;
    extract_prefix(path, "World", &inst.saved_dir())?;
    Ok(())
}

pub fn folder_size(dir: &Path) -> u64 {
    WalkDir::new(dir).into_iter().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(inst: &ServerInstance, name: &str) {
        std::fs::create_dir_all(inst.saved_dir()).unwrap();
        let c = rusqlite::Connection::open(inst.db_path()).unwrap();
        c.execute_batch(&format!("CREATE TABLE IF NOT EXISTS characters(char_name TEXT); DELETE FROM characters; INSERT INTO characters VALUES ('{name}');")).unwrap();
    }

    fn names(inst: &ServerInstance) -> Vec<String> {
        let c = rusqlite::Connection::open(inst.db_path()).unwrap();
        let mut s = c.prepare("SELECT char_name FROM characters").unwrap();
        s.query_map([], |r| r.get(0)).unwrap().flatten().collect()
    }

    #[test]
    fn hot_backup_verify_restore_round_trip() {
        let root = std::env::temp_dir().join(format!("cesm-bk-{}", uuid::Uuid::new_v4()));
        let mut inst = ServerInstance { install_dir: root.join("srv"), ..Default::default() };
        inst.mods = vec![ModEntry { workshop_id: "3745387835".into(), ..Default::default() }];
        world(&inst, "Conan");
        std::fs::create_dir_all(inst.config_dir()).unwrap();
        std::fs::write(inst.settings_path(), "[ServerSettings]\r\nAdminPassword=secret\r\nHarvestAmountMultiplier=2\r\n").unwrap();
        let b = create_blocking(&inst, &root.join("bk"), &root.join("cache"), &root.join("tmp"), "test", "manual", Some("123".into())).unwrap();
        assert!(b.verified, "integrity check on snapshot");
        assert_eq!(b.build.as_deref(), Some("123"));
        let rep = verify(&b.path, &root.join("tmp")).unwrap();
        assert!(rep.ok && rep.db_ok == Some(true), "{:?}", rep.mismatched);

        world(&inst, "Thoth-Amon");
        // A stale WAL next to the DB must be moved aside on restore.
        std::fs::write(format!("{}-wal", inst.db_path().display()), b"stale").unwrap();
        restore(&inst, &b.path, false).unwrap();
        assert_eq!(names(&inst), vec!["Conan"]);
        assert!(!PathBuf::from(format!("{}-wal", inst.db_path().display())).exists() || names(&inst) == vec!["Conan"]);
        assert!(inst.saved_dir().join("_quarantine").exists());
        assert_eq!(backup_mod_list(&b.path).unwrap()[0].workshop_id, "3745387835");
        let listed = list(&root.join("bk"));
        assert_eq!(listed.len(), 1);
        assert!(listed[0].verified);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn packs_strip_secrets() {
        let t = strip_secrets("ServerSettings.ini", "[ServerSettings]\r\nAdminPassword=secret\r\nPVPEnabled=True\r\n");
        assert!(t.contains("AdminPassword=\r\n") && t.contains("PVPEnabled=True"));
        let g = strip_secrets("Game.ini", "[RconPlugin]\r\nRconPassword=abc\r\nRconPort=25575\r\n");
        assert!(g.contains("RconPassword=\r\n"));
    }
}
