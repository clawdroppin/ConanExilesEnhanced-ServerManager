//! Read-only queries against the server's world database (`ConanSandbox\Saved\game_0.db`).
//!
//! Opened with `SQLITE_OPEN_READ_ONLY`, which is safe while the server runs (it uses WAL; readers
//! never block the writer). Schema verified on a fresh Enhanced 3.0.1 database (AGENTS.md §3):
//! `account(id, user, online, platformId)`, `characters(playerId, id, char_name, level, rank,
//! guild, isAlive, killerName, lastTimeOnline, killerId, lastServerTimeOnline)`,
//! `guilds(guildId, name, messageOfTheDay, owner, …)`, `game_events(…)`,
//! `serverPopulationRecordings(timeOfRecording, population)`, `dw_settings(name, value)`.

use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::Path;

fn open(db: &Path) -> AppResult<Connection> {
    if !db.exists() {
        return Err(AppError::msg("The world database doesn't exist yet – start the server once to create it."));
    }
    let c = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI)
        .map_err(|e| AppError::msg(format!("Can't open the world database: {e}")))?;
    c.busy_timeout(std::time::Duration::from_secs(5)).ok();
    Ok(c)
}

fn has_table(c: &Connection, t: &str) -> bool {
    c.query_row("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1", [t], |_| Ok(())).is_ok()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub id: i64,
    pub name: String,
    pub level: i64,
    pub clan_id: Option<i64>,
    pub clan: String,
    pub alive: bool,
    pub killer: String,
    /// Unix seconds.
    pub last_online: i64,
    pub account_id: String,
    /// Funcom/platform user id (matches RCON `listplayers` "User ID").
    pub user: String,
    pub platform_id: String,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clan {
    pub id: i64,
    pub name: String,
    pub motd: String,
    pub owner_id: i64,
    pub owner: String,
    pub members: i64,
    pub max_level: i64,
    pub last_active: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    pub time: i64,
    pub event_type: i64,
    pub object: String,
    pub owner: String,
    pub owner_clan: String,
    pub causer: String,
    pub causer_clan: String,
    pub x: i64,
    pub y: i64,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSummary {
    pub size_bytes: u64,
    pub characters: i64,
    pub alive: i64,
    pub accounts: i64,
    pub clans: i64,
    pub buildings: i64,
    pub building_pieces: i64,
    pub placeables: i64,
    pub thralls_and_pets: i64,
    pub db_version: String,
    pub server_runtime_secs: f64,
}

pub fn characters(db: &Path) -> AppResult<Vec<Character>> {
    let c = open(db)?;
    let mut st = c
        .prepare(
            "SELECT ch.id, ch.char_name, COALESCE(ch.level,0), ch.guild, COALESCE(g.name,''), COALESCE(ch.isAlive,0),
                    COALESCE(ch.killerName,''), COALESCE(ch.lastTimeOnline,0), COALESCE(ch.playerId,''),
                    COALESCE(a.user,''), COALESCE(a.platformId,''), COALESCE(a.online,0)
             FROM characters ch
             LEFT JOIN guilds g ON g.guildId = ch.guild
             LEFT JOIN account a ON CAST(a.id AS TEXT) = CAST(ch.playerId AS TEXT)
             ORDER BY ch.lastTimeOnline DESC",
        )
        .map_err(|e| AppError::msg(format!("Unexpected world database layout: {e}")))?;
    let rows = st
        .query_map([], |r| {
            Ok(Character {
                id: r.get(0)?,
                name: r.get(1)?,
                level: r.get(2)?,
                clan_id: r.get(3)?,
                clan: r.get(4)?,
                alive: r.get::<_, i64>(5)? != 0,
                killer: r.get(6)?,
                last_online: r.get(7)?,
                account_id: r.get::<_, rusqlite::types::Value>(8).map(value_text)?,
                user: r.get(9)?,
                platform_id: r.get(10)?,
                online: r.get::<_, i64>(11)? != 0,
            })
        })
        .map_err(|e| AppError::msg(e.to_string()))?;
    Ok(rows.flatten().collect())
}

fn value_text(v: rusqlite::types::Value) -> String {
    match v {
        rusqlite::types::Value::Integer(i) => i.to_string(),
        rusqlite::types::Value::Real(f) => f.to_string(),
        rusqlite::types::Value::Text(t) => t,
        _ => String::new(),
    }
}

pub fn clans(db: &Path) -> AppResult<Vec<Clan>> {
    let c = open(db)?;
    let mut st = c
        .prepare(
            "SELECT g.guildId, COALESCE(g.name,''), COALESCE(g.messageOfTheDay,''), g.owner, COALESCE(o.char_name,''),
                    (SELECT COUNT(*) FROM characters m WHERE m.guild = g.guildId),
                    (SELECT COALESCE(MAX(level),0) FROM characters m WHERE m.guild = g.guildId),
                    (SELECT COALESCE(MAX(lastTimeOnline),0) FROM characters m WHERE m.guild = g.guildId)
             FROM guilds g LEFT JOIN characters o ON o.id = g.owner ORDER BY 6 DESC, 2",
        )
        .map_err(|e| AppError::msg(format!("Unexpected world database layout: {e}")))?;
    let rows = st
        .query_map([], |r| {
            Ok(Clan {
                id: r.get(0)?,
                name: r.get(1)?,
                motd: r.get(2)?,
                owner_id: r.get(3)?,
                owner: r.get(4)?,
                members: r.get(5)?,
                max_level: r.get(6)?,
                last_active: r.get(7)?,
            })
        })
        .map_err(|e| AppError::msg(e.to_string()))?;
    Ok(rows.flatten().collect())
}

/// Newest entries of the in-game event log (kills, building destruction, …).
pub fn events(db: &Path, limit: u32, filter: &str) -> AppResult<Vec<GameEvent>> {
    let c = open(db)?;
    if !has_table(&c, "game_events") {
        return Ok(vec![]);
    }
    let like = format!("%{}%", filter.trim());
    let mut st = c
        .prepare(
            "SELECT worldTime, COALESCE(eventType,0), COALESCE(objectName,''), COALESCE(ownerName,''), COALESCE(ownerGuildName,''),
                    COALESCE(causerName,''), COALESCE(causerGuildName,''), x, y, COALESCE(details,'')
             FROM game_events
             WHERE ?1 = '%%' OR objectName LIKE ?1 OR ownerName LIKE ?1 OR causerName LIKE ?1 OR ownerGuildName LIKE ?1 OR causerGuildName LIKE ?1
             ORDER BY worldTime DESC LIMIT ?2",
        )
        .map_err(|e| AppError::msg(e.to_string()))?;
    let rows = st
        .query_map(rusqlite::params![like, limit.min(5000)], |r| {
            Ok(GameEvent {
                time: r.get(0)?,
                event_type: r.get(1)?,
                object: r.get(2)?,
                owner: r.get(3)?,
                owner_clan: r.get(4)?,
                causer: r.get(5)?,
                causer_clan: r.get(6)?,
                x: r.get(7)?,
                y: r.get(8)?,
                details: r.get(9)?,
            })
        })
        .map_err(|e| AppError::msg(e.to_string()))?;
    Ok(rows.flatten().collect())
}

pub fn summary(db: &Path) -> AppResult<WorldSummary> {
    let c = open(db)?;
    let count = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    let setting = |k: &str| -> String { c.query_row("SELECT value FROM dw_settings WHERE name = ?1", [k], |r| r.get(0)).unwrap_or_default() };
    // Main database file + WAL = what's actually on disk.
    let size = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0) + std::fs::metadata(db.with_extension("db-wal")).map(|m| m.len()).unwrap_or(0);
    Ok(WorldSummary {
        size_bytes: size,
        characters: count("SELECT COUNT(*) FROM characters"),
        alive: count("SELECT COUNT(*) FROM characters WHERE isAlive = 1"),
        accounts: count("SELECT COUNT(*) FROM account"),
        clans: count("SELECT COUNT(*) FROM guilds"),
        buildings: count("SELECT COUNT(*) FROM buildings"),
        building_pieces: count("SELECT COUNT(*) FROM building_instances"),
        placeables: count("SELECT COUNT(*) FROM actor_position"),
        thralls_and_pets: count("SELECT COUNT(*) FROM actor_position WHERE class LIKE '%Humanoid%' OR class LIKE '%Pet%' OR class LIKE '%Thrall%'"),
        db_version: setting("version"),
        server_runtime_secs: setting("serverruntime").parse().unwrap_or(0.0),
    })
}

/// Run a read-only query (SQL console). Writes are impossible: the connection is read-only.
pub fn query(db: &Path, sql: &str, max_rows: usize) -> AppResult<super::rcon::SqlTable> {
    let c = open(db)?;
    let mut st = c.prepare(sql).map_err(|e| AppError::msg(e.to_string()))?;
    let columns: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
    let n = columns.len();
    let mut rows = vec![];
    let mut q = st.query([]).map_err(|e| AppError::msg(e.to_string()))?;
    while let Some(r) = q.next().map_err(|e| AppError::msg(e.to_string()))? {
        let mut row = Vec::with_capacity(n);
        for i in 0..n {
            let v: rusqlite::types::Value = r.get(i).unwrap_or(rusqlite::types::Value::Null);
            row.push(match v {
                rusqlite::types::Value::Null => "NULL".into(),
                rusqlite::types::Value::Blob(b) => format!("<blob {} bytes>", b.len()),
                other => value_text(other),
            });
        }
        rows.push(row);
        if rows.len() >= max_rows {
            break;
        }
    }
    Ok(super::rcon::SqlTable { columns, rows })
}

/// Hot, consistent copy of the live database using SQLite's online-backup API (works while the
/// server is writing), followed by `PRAGMA integrity_check` on the copy. Returns whether the copy
/// is healthy.
pub fn snapshot(db: &Path, dest: &Path) -> AppResult<bool> {
    let src = open(db)?;
    let mut dst = Connection::open(dest).map_err(|e| AppError::msg(format!("Can't create snapshot: {e}")))?;
    {
        let b = rusqlite::backup::Backup::new(&src, &mut dst).map_err(|e| AppError::msg(format!("Snapshot failed: {e}")))?;
        b.run_to_completion(256, std::time::Duration::from_millis(5), None).map_err(|e| AppError::msg(format!("Snapshot failed: {e}")))?;
    }
    let ok: String = dst.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap_or_else(|_| "error".into());
    // Make the copy a self-contained single file (no WAL side files inside the backup).
    let _ = dst.pragma_update(None, "journal_mode", "DELETE");
    Ok(ok == "ok")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(dir: &Path) -> std::path::PathBuf {
        let p = dir.join("game_0.db");
        let c = Connection::open(&p).unwrap();
        c.execute_batch(
            "CREATE TABLE account (id INTEGER PRIMARY KEY AUTOINCREMENT, user TEXT UNIQUE, online BOOL NOT NULL DEFAULT 0, platformId TEXT NOT NULL DEFAULT '');
             CREATE TABLE characters (playerId TEXT, id BIGINT NOT NULL UNIQUE PRIMARY KEY, char_name TEXT NOT NULL, level INTEGER, rank INTEGER, guild BIGINT, isAlive BOOLEAN, killerName TEXT, lastTimeOnline INTEGER, killerId TEXT, lastServerTimeOnline REAL);
             CREATE TABLE guilds(guildId BIGINT NOT NULL PRIMARY KEY, name TEXT, messageOfTheDay TEXT, owner BIGINT NOT NULL, nameLastChangedBy BIGINT NOT NULL DEFAULT -1, motdLastChangedBy BIGINT NOT NULL DEFAULT -1, rescueCooldown BIGINT NOT NULL DEFAULT 0, emblemInfo BLOB);
             CREATE TABLE buildings(object_id bigint NOT NULL, owner_id bigint NOT NULL);
             CREATE TABLE building_instances(object_id bigint NOT NULL, instance_id bigint NOT NULL, class TEXT NOT NULL, transform1 BLOB NOT NULL, transform2 BLOB NOT NULL, building_flags INTEGER, stability_loss_multiplier real NOT NULL DEFAULT 1);
             CREATE TABLE actor_position(class TEXT, map text, id bigint NOT NULL, x double precision, y double precision, z double precision, sx double precision, sy double precision, sz double precision, rx double precision, ry double precision, rz double precision, rw double precision);
             CREATE TABLE dw_settings(name TEXT, value TEXT);
             INSERT INTO dw_settings VALUES ('version','30'),('serverruntime','60.5');
             INSERT INTO account(id, user, online, platformId) VALUES (5, 'FUNCOM-1', 1, '7656');
             INSERT INTO guilds(guildId, name, messageOfTheDay, owner) VALUES (100, 'Cimmerians', 'By Crom', 1);
             INSERT INTO characters VALUES ('5', 1, 'Conan', 60, 0, 100, 1, '', 1791616000, '', 0);
             INSERT INTO characters VALUES ('6', 2, 'Valeria', 42, 0, 100, 0, 'Thoth', 1791610000, '', 0);",
        )
        .unwrap();
        p
    }

    #[test]
    fn reads_players_clans_and_summary() {
        let dir = std::env::temp_dir().join(format!("cesm-gdb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = fixture(&dir);
        let ch = characters(&db).unwrap();
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].name, "Conan");
        assert_eq!(ch[0].clan, "Cimmerians");
        assert_eq!(ch[0].user, "FUNCOM-1");
        assert!(ch[0].online);
        assert_eq!(ch[1].killer, "Thoth");
        let cl = clans(&db).unwrap();
        assert_eq!(cl[0].members, 2);
        assert_eq!(cl[0].owner, "Conan");
        let s = summary(&db).unwrap();
        assert_eq!((s.characters, s.alive, s.clans), (2, 1, 1));
        assert_eq!(s.db_version, "30");
        let q = query(&db, "SELECT char_name FROM characters ORDER BY id", 10).unwrap();
        assert_eq!(q.rows, vec![vec!["Conan".to_string()], vec!["Valeria".to_string()]]);
        assert!(query(&db, "DELETE FROM characters", 10).is_err(), "read-only");
        let snap = dir.join("snap.db");
        assert!(snapshot(&db, &snap).unwrap());
        assert_eq!(characters(&snap).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
