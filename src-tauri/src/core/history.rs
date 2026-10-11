//! `manager.db` – the manager's own SQLite database: metric rollups, player sessions, admin
//! notes and moderation history, the event timeline, the audit log, build history and the
//! Workshop metadata cache. Schema in docs/DESIGN.md §6.2.
//!
//! A failure here must never take the app down: if the database can't be opened the manager runs
//! without history (every call becomes a no-op) and logs why.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS metrics (
  server_id TEXT NOT NULL, ts INTEGER NOT NULL,
  cpu REAL, mem INTEGER, read_bps INTEGER, write_bps INTEGER, players INTEGER, db_size INTEGER,
  PRIMARY KEY (server_id, ts)
);
CREATE TABLE IF NOT EXISTS sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL, user_id TEXT NOT NULL, platform_id TEXT,
  player_name TEXT, char_name TEXT, joined_at INTEGER NOT NULL, left_at INTEGER
);
CREATE INDEX IF NOT EXISTS sessions_user ON sessions(server_id, user_id);
CREATE TABLE IF NOT EXISTS player_notes (
  server_id TEXT NOT NULL, user_id TEXT NOT NULL, note TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL,
  PRIMARY KEY (server_id, user_id)
);
CREATE TABLE IF NOT EXISTS moderation (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL, user_id TEXT NOT NULL, player_name TEXT, action TEXT NOT NULL,
  reason TEXT, actor TEXT NOT NULL DEFAULT 'admin', ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT, ts INTEGER NOT NULL, level TEXT NOT NULL, kind TEXT NOT NULL, message TEXT NOT NULL, data TEXT
);
CREATE INDEX IF NOT EXISTS events_ts ON events(server_id, ts);
CREATE TABLE IF NOT EXISTS audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts INTEGER NOT NULL, actor TEXT NOT NULL, server_id TEXT, action TEXT NOT NULL, detail TEXT
);
CREATE TABLE IF NOT EXISTS builds (
  server_id TEXT NOT NULL, build_id TEXT NOT NULL, branch TEXT NOT NULL, ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS workshop_items (
  id TEXT PRIMARY KEY, json TEXT NOT NULL, fetched_at INTEGER NOT NULL
);
"#;

pub struct History {
    db: Mutex<Option<Connection>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricRow {
    pub ts: i64,
    pub cpu: f64,
    pub mem: i64,
    pub read_bps: i64,
    pub write_bps: i64,
    pub players: i64,
    pub db_size: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRow {
    pub id: i64,
    pub server_id: Option<String>,
    pub ts: i64,
    pub level: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSummary {
    pub user_id: String,
    pub platform_id: String,
    pub player_name: String,
    pub char_name: String,
    pub sessions: i64,
    pub playtime_secs: i64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub online: bool,
    pub note: String,
    pub warnings: i64,
    pub kicks: i64,
    pub bans: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationRow {
    pub id: i64,
    pub user_id: String,
    pub player_name: String,
    pub action: String,
    pub reason: String,
    pub actor: String,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRow {
    pub id: i64,
    pub ts: i64,
    pub actor: String,
    pub server_id: Option<String>,
    pub action: String,
    pub detail: String,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl History {
    pub fn open(path: &Path) -> Self {
        let conn = (|| -> rusqlite::Result<Connection> {
            let c = Connection::open(path)?;
            c.pragma_update(None, "journal_mode", "WAL")?;
            c.pragma_update(None, "synchronous", "NORMAL")?;
            c.execute_batch(SCHEMA)?;
            let v: Option<i64> = c.query_row("SELECT version FROM schema_version", [], |r| r.get(0)).optional()?;
            if v.is_none() {
                c.execute("INSERT INTO schema_version(version) VALUES (?1)", [SCHEMA_VERSION])?;
            }
            Ok(c)
        })();
        match conn {
            Ok(c) => Self { db: Mutex::new(Some(c)) },
            Err(e) => {
                tracing::error!("manager.db unavailable ({e}); running without history");
                Self { db: Mutex::new(None) }
            }
        }
    }

    #[cfg(test)]
    pub fn memory() -> Self {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(SCHEMA).unwrap();
        Self { db: Mutex::new(Some(c)) }
    }

    fn with<R: Default>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<R>) -> R {
        let Ok(g) = self.db.lock() else { return R::default() };
        match g.as_ref() {
            Some(c) => f(c).unwrap_or_else(|e| {
                tracing::warn!("manager.db: {e}");
                R::default()
            }),
            None => R::default(),
        }
    }

    // ------------------------------------------------------------------------------ write

    pub fn event(&self, server_id: Option<&str>, level: &str, kind: &str, message: &str) {
        self.with(|c| {
            c.execute(
                "INSERT INTO events(server_id, ts, level, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![server_id, now(), level, kind, message],
            )
            .map(|_| ())
        })
    }

    pub fn audit(&self, server_id: Option<&str>, action: &str, detail: &str) {
        self.with(|c| {
            c.execute("INSERT INTO audit(ts, actor, server_id, action, detail) VALUES (?1, 'admin', ?2, ?3, ?4)", params![now(), server_id, action, detail])
                .map(|_| ())
        })
    }

    pub fn metric(&self, server_id: &str, row: &MetricRow) {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO metrics(server_id, ts, cpu, mem, read_bps, write_bps, players, db_size) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![server_id, row.ts, row.cpu, row.mem, row.read_bps, row.write_bps, row.players, row.db_size],
            )
            .map(|_| ())
        })
    }

    pub fn session_open(&self, server_id: &str, p: &crate::models::OnlinePlayer) {
        self.with(|c| {
            c.execute(
                "INSERT INTO sessions(server_id, user_id, platform_id, player_name, char_name, joined_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![server_id, p.user_id, p.platform_id, p.player_name, p.char_name, now()],
            )
            .map(|_| ())
        })
    }

    pub fn session_close(&self, server_id: &str, user_id: &str) {
        self.with(|c| {
            c.execute("UPDATE sessions SET left_at = ?3 WHERE server_id = ?1 AND user_id = ?2 AND left_at IS NULL", params![server_id, user_id, now()])
                .map(|_| ())
        })
    }

    /// Close every open session of a server (it stopped or the manager restarted).
    pub fn close_all_sessions(&self, server_id: &str) {
        self.with(|c| c.execute("UPDATE sessions SET left_at = ?2 WHERE server_id = ?1 AND left_at IS NULL", params![server_id, now()]).map(|_| ()))
    }

    pub fn set_note(&self, server_id: &str, user_id: &str, note: &str) {
        self.with(|c| {
            c.execute(
                "INSERT INTO player_notes(server_id, user_id, note, updated_at) VALUES (?1,?2,?3,?4)
                 ON CONFLICT(server_id, user_id) DO UPDATE SET note = excluded.note, updated_at = excluded.updated_at",
                params![server_id, user_id, note, now()],
            )
            .map(|_| ())
        })
    }

    pub fn moderation(&self, server_id: &str, user_id: &str, player_name: &str, action: &str, reason: &str) {
        self.with(|c| {
            c.execute(
                "INSERT INTO moderation(server_id, user_id, player_name, action, reason, ts) VALUES (?1,?2,?3,?4,?5,?6)",
                params![server_id, user_id, player_name, action, reason, now()],
            )
            .map(|_| ())
        })
    }

    pub fn build(&self, server_id: &str, build_id: &str, branch: &str) {
        self.with(|c| {
            let last: Option<String> =
                c.query_row("SELECT build_id FROM builds WHERE server_id = ?1 ORDER BY ts DESC LIMIT 1", [server_id], |r| r.get(0)).optional()?;
            if last.as_deref() != Some(build_id) {
                c.execute("INSERT INTO builds(server_id, build_id, branch, ts) VALUES (?1,?2,?3,?4)", params![server_id, build_id, branch, now()])?;
            }
            Ok(())
        })
    }

    pub fn cache_workshop(&self, id: &str, json: &str) {
        self.with(|c| {
            c.execute("INSERT OR REPLACE INTO workshop_items(id, json, fetched_at) VALUES (?1,?2,?3)", params![id, json, now()]).map(|_| ())
        })
    }

    pub fn cached_workshop(&self, id: &str, max_age_ms: i64) -> Option<String> {
        self.with(|c| {
            c.query_row("SELECT json FROM workshop_items WHERE id = ?1 AND fetched_at > ?2", params![id, now() - max_age_ms], |r| r.get(0)).optional()
        })
    }

    /// Drop old metrics; keep everything else.
    pub fn prune(&self, metrics_days: u32) {
        let cutoff = now() - metrics_days.max(1) as i64 * 86_400_000;
        self.with(|c| c.execute("DELETE FROM metrics WHERE ts < ?1", [cutoff]).map(|_| ()))
    }

    // ------------------------------------------------------------------------------- read

    pub fn metrics(&self, server_id: &str, since: i64) -> Vec<MetricRow> {
        self.with(|c| {
            let mut st = c.prepare("SELECT ts, cpu, mem, read_bps, write_bps, players, db_size FROM metrics WHERE server_id = ?1 AND ts >= ?2 ORDER BY ts")?;
            let rows = st.query_map(params![server_id, since], |r| {
                Ok(MetricRow {
                    ts: r.get(0)?,
                    cpu: r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                    mem: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    read_bps: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    write_bps: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                    players: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    db_size: r.get::<_, Option<i64>>(6)?.unwrap_or(0),
                })
            })?;
            rows.collect()
        })
    }

    pub fn events(&self, server_id: Option<&str>, limit: u32) -> Vec<EventRow> {
        self.with(|c| {
            let map = |r: &rusqlite::Row| {
                Ok(EventRow { id: r.get(0)?, server_id: r.get(1)?, ts: r.get(2)?, level: r.get(3)?, kind: r.get(4)?, message: r.get(5)? })
            };
            match server_id {
                Some(s) => {
                    let mut st = c.prepare("SELECT id, server_id, ts, level, kind, message FROM events WHERE server_id = ?1 ORDER BY ts DESC LIMIT ?2")?;
                    let v = st.query_map(params![s, limit], map)?.collect();
                    v
                }
                None => {
                    let mut st = c.prepare("SELECT id, server_id, ts, level, kind, message FROM events ORDER BY ts DESC LIMIT ?1")?;
                    let v = st.query_map(params![limit], map)?.collect();
                    v
                }
            }
        })
    }

    pub fn audit_log(&self, limit: u32) -> Vec<AuditRow> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, ts, actor, server_id, action, COALESCE(detail,'') FROM audit ORDER BY ts DESC LIMIT ?1")?;
            let v = st
                .query_map([limit], |r| Ok(AuditRow { id: r.get(0)?, ts: r.get(1)?, actor: r.get(2)?, server_id: r.get(3)?, action: r.get(4)?, detail: r.get(5)? }))?
                .collect();
            v
        })
    }

    /// One row per player ever seen on the server, with playtime and moderation counts.
    pub fn players(&self, server_id: &str) -> Vec<PlayerSummary> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT s.user_id,
                        (SELECT platform_id FROM sessions x WHERE x.server_id = s.server_id AND x.user_id = s.user_id ORDER BY joined_at DESC LIMIT 1),
                        (SELECT player_name FROM sessions x WHERE x.server_id = s.server_id AND x.user_id = s.user_id ORDER BY joined_at DESC LIMIT 1),
                        (SELECT char_name FROM sessions x WHERE x.server_id = s.server_id AND x.user_id = s.user_id ORDER BY joined_at DESC LIMIT 1),
                        COUNT(*),
                        SUM(COALESCE(s.left_at, ?2) - s.joined_at) / 1000,
                        MIN(s.joined_at), MAX(COALESCE(s.left_at, ?2)),
                        MAX(CASE WHEN s.left_at IS NULL THEN 1 ELSE 0 END),
                        COALESCE((SELECT note FROM player_notes n WHERE n.server_id = s.server_id AND n.user_id = s.user_id), ''),
                        (SELECT COUNT(*) FROM moderation m WHERE m.server_id = s.server_id AND m.user_id = s.user_id AND m.action = 'warn'),
                        (SELECT COUNT(*) FROM moderation m WHERE m.server_id = s.server_id AND m.user_id = s.user_id AND m.action = 'kick'),
                        (SELECT COUNT(*) FROM moderation m WHERE m.server_id = s.server_id AND m.user_id = s.user_id AND m.action = 'ban')
                 FROM sessions s WHERE s.server_id = ?1 GROUP BY s.user_id ORDER BY MAX(COALESCE(s.left_at, ?2)) DESC",
            )?;
            let v = st
                .query_map(params![server_id, now()], |r| {
                    Ok(PlayerSummary {
                        user_id: r.get(0)?,
                        platform_id: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        player_name: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        char_name: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        sessions: r.get(4)?,
                        playtime_secs: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                        first_seen: r.get(6)?,
                        last_seen: r.get(7)?,
                        online: r.get::<_, i64>(8)? == 1,
                        note: r.get(9)?,
                        warnings: r.get(10)?,
                        kicks: r.get(11)?,
                        bans: r.get(12)?,
                    })
                })?
                .collect();
            v
        })
    }

    pub fn moderation_log(&self, server_id: &str, user_id: Option<&str>) -> Vec<ModerationRow> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, user_id, COALESCE(player_name,''), action, COALESCE(reason,''), actor, ts FROM moderation
                 WHERE server_id = ?1 AND (?2 IS NULL OR user_id = ?2) ORDER BY ts DESC LIMIT 500",
            )?;
            let v = st
                .query_map(params![server_id, user_id], |r| {
                    Ok(ModerationRow {
                        id: r.get(0)?,
                        user_id: r.get(1)?,
                        player_name: r.get(2)?,
                        action: r.get(3)?,
                        reason: r.get(4)?,
                        actor: r.get(5)?,
                        ts: r.get(6)?,
                    })
                })?
                .collect();
            v
        })
    }

    pub fn builds(&self, server_id: &str) -> Vec<(String, String, i64)> {
        self.with(|c| {
            let mut st = c.prepare("SELECT build_id, branch, ts FROM builds WHERE server_id = ?1 ORDER BY ts DESC LIMIT 100")?;
            let v = st.query_map([server_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect();
            v
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::OnlinePlayer;

    #[test]
    fn sessions_and_playtime() {
        let h = History::memory();
        let p = OnlinePlayer { user_id: "U1".into(), player_name: "Arnold".into(), char_name: "Conan".into(), ..Default::default() };
        h.session_open("s", &p);
        h.moderation("s", "U1", "Arnold", "warn", "spam");
        h.set_note("s", "U1", "nice guy");
        let list = h.players("s");
        assert_eq!(list.len(), 1);
        assert!(list[0].online);
        assert_eq!(list[0].warnings, 1);
        assert_eq!(list[0].note, "nice guy");
        h.session_close("s", "U1");
        assert!(!h.players("s")[0].online);
    }

    #[test]
    fn builds_dedupe_and_events() {
        let h = History::memory();
        h.build("s", "1", "public");
        h.build("s", "1", "public");
        h.build("s", "2", "public");
        assert_eq!(h.builds("s").len(), 2);
        h.event(Some("s"), "info", "test", "hello");
        assert_eq!(h.events(Some("s"), 10)[0].message, "hello");
        h.audit(Some("s"), "start", "");
        assert_eq!(h.audit_log(10).len(), 1);
    }
}
