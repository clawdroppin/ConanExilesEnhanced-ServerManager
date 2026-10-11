//! RCON client for the Conan Exiles Enhanced server (`Game.ini [RconPlugin]`).
//!
//! Source-RCON framing (`<len i32><id i32><type i32><body>\0\0`) with Conan's quirks, all
//! verified against a real server (AGENTS.md §3):
//! * the auth reply is a single type-2 packet with id 0 (`Authenticated.`) or -1 (`Authentication
//!   failed.`), after which a failed connection is closed by the server;
//! * every command reply carries the id of the *previous* request, so replies are matched by
//!   order – exactly one command is in flight per connection;
//! * an empty command is an error ("Couldn't parse the command"), so the usual empty-packet
//!   sentinel trick can't be used to detect the end of a multi-packet reply;
//! * a rate limit ("karma", `RconMaxKarma`) answers bursts with "Too many commands";
//! * the server waits for connected RCON clients to drop before it finishes `shutdown`, so the
//!   connection is closed straight after sending it.

use crate::error::{AppError, AppResult};
use crate::models::OnlinePlayer;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

const TYPE_AUTH: i32 = 3;
const TYPE_EXEC: i32 = 2;
const IO_TIMEOUT: Duration = Duration::from_secs(8);
/// Bodies at or above this size may continue in another packet.
const SPLIT_HINT: usize = 4000;

pub struct RconConn {
    stream: TcpStream,
    next_id: i32,
}

async fn write_packet(s: &mut TcpStream, id: i32, kind: i32, body: &str) -> AppResult<()> {
    let b = body.as_bytes();
    let len = (b.len() + 10) as i32;
    let mut buf = Vec::with_capacity(b.len() + 14);
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&id.to_le_bytes());
    buf.extend_from_slice(&kind.to_le_bytes());
    buf.extend_from_slice(b);
    buf.extend_from_slice(&[0, 0]);
    tokio::time::timeout(IO_TIMEOUT, s.write_all(&buf)).await.map_err(|_| AppError::msg("RCON write timed out"))??;
    Ok(())
}

async fn read_packet(s: &mut TcpStream, timeout: Duration) -> AppResult<(i32, i32, String)> {
    let mut hdr = [0u8; 4];
    tokio::time::timeout(timeout, s.read_exact(&mut hdr)).await.map_err(|_| AppError::msg("RCON read timed out"))??;
    let len = i32::from_le_bytes(hdr);
    if !(8..=1_048_576).contains(&len) {
        return Err(AppError::msg(format!("RCON: bad packet length {len}")));
    }
    let mut data = vec![0u8; len as usize];
    tokio::time::timeout(timeout, s.read_exact(&mut data)).await.map_err(|_| AppError::msg("RCON read timed out"))??;
    let id = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let kind = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let body = String::from_utf8_lossy(&data[8..]).trim_end_matches('\0').to_string();
    Ok((id, kind, body))
}

impl RconConn {
    pub async fn connect(host: &str, port: u16, password: &str) -> AppResult<Self> {
        let addr = format!("{host}:{port}");
        let stream = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(&addr))
            .await
            .map_err(|_| AppError::msg(format!("RCON: no answer on {addr}")))?
            .map_err(|e| AppError::msg(format!("RCON: can't connect to {addr} ({e})")))?;
        let _ = stream.set_nodelay(true);
        let mut c = Self { stream, next_id: 1 };
        write_packet(&mut c.stream, c.next_id, TYPE_AUTH, password).await?;
        c.next_id += 1;
        let (id, _, body) = read_packet(&mut c.stream, IO_TIMEOUT)
            .await
            .map_err(|_| AppError::msg("RCON: the server closed the connection – wrong password?"))?;
        if id == -1 || body.to_lowercase().contains("failed") {
            return Err(AppError::msg("RCON authentication failed – check the RCON password."));
        }
        Ok(c)
    }

    /// Run one command and return its full reply text.
    pub async fn exec(&mut self, command: &str) -> AppResult<String> {
        let command = command.trim();
        if command.is_empty() {
            return Err(AppError::msg("Empty command"));
        }
        // Conan's "karma" rate limit: a burst of commands is answered with "Too many commands,
        // try again later." and, if it continues, the connection is dropped and the IP denied for
        // about a minute (verified). Back off and retry instead of hammering it.
        let mut body = String::new();
        for attempt in 0..6 {
            write_packet(&mut self.stream, self.next_id, TYPE_EXEC, command).await?;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            let (_, _, b) = read_packet(&mut self.stream, Duration::from_secs(20)).await?;
            if b.contains("Too many commands") && attempt < 5 {
                tokio::time::sleep(Duration::from_secs(3 + attempt as u64 * 2)).await;
                continue;
            }
            body = b;
            break;
        }
        // Large replies may be split; keep reading while packets arrive back-to-back.
        let mut last = body.len();
        while last >= SPLIT_HINT {
            match read_packet(&mut self.stream, Duration::from_millis(400)).await {
                Ok((_, _, more)) => {
                    last = more.len();
                    body.push_str(&more);
                }
                Err(_) => break,
            }
        }
        Ok(body.trim_end().to_string())
    }

    /// Send a command without waiting for (or caring about) a reply, then close.
    pub async fn fire_and_close(mut self, command: &str) -> AppResult<()> {
        write_packet(&mut self.stream, self.next_id, TYPE_EXEC, command).await?;
        // Give the server a moment to read it before the socket goes away.
        let _ = read_packet(&mut self.stream, Duration::from_millis(1500)).await;
        let _ = self.stream.shutdown().await;
        Ok(())
    }
}

/// One lazily-connected RCON session per server, shared by the supervisor and commands.
#[derive(Default)]
pub struct RconPool {
    conns: Mutex<HashMap<String, Arc<Mutex<Option<RconConn>>>>>,
}

#[derive(Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub password: String,
}

impl RconPool {
    async fn slot(&self, id: &str) -> Arc<Mutex<Option<RconConn>>> {
        self.conns.lock().await.entry(id.to_string()).or_default().clone()
    }

    /// Execute on the server's session, connecting (and reconnecting once) as needed.
    pub async fn exec(&self, id: &str, t: &Target, command: &str) -> AppResult<String> {
        let slot = self.slot(id).await;
        let mut guard = slot.lock().await;
        for attempt in 0..2 {
            if guard.is_none() {
                *guard = Some(RconConn::connect(&t.host, t.port, &t.password).await?);
            }
            match guard.as_mut().unwrap().exec(command).await {
                Ok(r) => return Ok(r),
                Err(e) => {
                    *guard = None;
                    if attempt == 1 {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!()
    }

    /// Drop the session (before shutdown, after the server stops, or when settings change).
    pub async fn disconnect(&self, id: &str) {
        let slot = self.slot(id).await;
        let mut g = slot.lock().await;
        if let Some(mut c) = g.take() {
            let _ = c.stream.shutdown().await;
        }
    }

    /// Clean shutdown: close the shared session first (the server waits for clients to drop),
    /// then send `shutdown` on a fresh connection and hang up immediately.
    pub async fn shutdown_server(&self, id: &str, t: &Target) -> AppResult<()> {
        self.disconnect(id).await;
        let c = RconConn::connect(&t.host, t.port, &t.password).await?;
        c.fire_and_close("shutdown").await
    }
}

// ------------------------------------------------------------------------------------ parsers

/// `listplayers` → rows. Format (verified header):
/// `Idx | Char name | Player name | User ID | Platform ID | Platform Name`
pub fn parse_listplayers(text: &str) -> Vec<OnlinePlayer> {
    text.lines()
        .filter(|l| l.contains('|'))
        .filter(|l| !l.trim_start().to_lowercase().starts_with("idx"))
        .filter_map(|l| {
            let c: Vec<String> = l.split('|').map(|s| s.trim().to_string()).collect();
            if c.len() < 4 {
                return None;
            }
            let idx = c[0].trim_start_matches('#').trim().parse().ok()?;
            Some(OnlinePlayer {
                idx,
                char_name: c.get(1).cloned().unwrap_or_default(),
                player_name: c.get(2).cloned().unwrap_or_default(),
                user_id: c.get(3).cloned().unwrap_or_default(),
                platform_id: c.get(4).cloned().unwrap_or_default(),
                platform: c.get(5).cloned().unwrap_or_default(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SqlTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listplayers_parsing() {
        let empty = "Idx | Char name | Player name | User ID | Platform ID | Platform Name\n";
        assert!(parse_listplayers(empty).is_empty());
        let one = "Idx | Char name | Player name | User ID | Platform ID | Platform Name\n  0 | Conan | Arnold | ABC123 | 76561198000000000 | Steam\n";
        let p = parse_listplayers(one);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].char_name, "Conan");
        assert_eq!(p[0].user_id, "ABC123");
        assert_eq!(p[0].platform, "Steam");
    }
}
