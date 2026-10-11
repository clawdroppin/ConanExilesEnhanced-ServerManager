//! Lossless Unreal-style INI document.
//!
//! Every line (comments, blank lines, unknown keys, ordering, array syntax `+Key=` / `-Key=` /
//! `!Key=` and repeated keys) survives a parse → edit → write round trip. Only the keys we
//! explicitly `set` / `remove` change. Conan's server rewrites ServerSettings.ini itself (on RCON
//! `SetServerSetting` and on settings upgrades – it keeps `ConfigVersion` / `BlueprintConfigVersion`
//! and a `;METADATA=` header in it), so never regex-edit these files.

use crate::error::AppResult;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
enum Line {
    Section(String, String), // name, raw
    Pair { raw: String, key: String, value: String },
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub struct IniDoc {
    lines: Vec<Line>,
    newline: String,
}

fn parse_line(raw: &str) -> Line {
    let t = raw.trim();
    if t.starts_with('[') && t.ends_with(']') && t.len() >= 2 {
        return Line::Section(t[1..t.len() - 1].trim().to_string(), raw.to_string());
    }
    if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
        return Line::Other(raw.to_string());
    }
    match t.find('=') {
        Some(eq) if eq > 0 => Line::Pair { raw: raw.to_string(), key: t[..eq].trim().to_string(), value: t[eq + 1..].to_string() },
        _ => Line::Other(raw.to_string()),
    }
}

/// Strip UE array prefixes so `+AdminList` and `AdminList` compare equal.
pub fn base_key(k: &str) -> &str {
    k.trim_start_matches(['+', '-', '!', '.'])
}

impl IniDoc {
    pub fn parse(text: &str) -> Self {
        // Unreal writes CRLF on Windows; normalise to it.
        let newline = "\r\n".to_string();
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let lines = if text.is_empty() { vec![] } else { text.split('\n').map(|l| parse_line(l.trim_end_matches('\r'))).collect() };
        let mut doc = Self { lines, newline };
        // Drop the phantom empty line produced by a trailing newline; to_text adds it back.
        if matches!(doc.lines.last(), Some(Line::Other(s)) if s.is_empty()) {
            doc.lines.pop();
        }
        doc
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for l in &self.lines {
            match l {
                Line::Section(_, raw) | Line::Other(raw) | Line::Pair { raw, .. } => out.push_str(raw),
            }
            out.push_str(&self.newline);
        }
        out
    }

    fn section_range(&self, section: &str) -> Option<(usize, usize)> {
        let start = self.lines.iter().position(|l| matches!(l, Line::Section(n, _) if n.eq_ignore_ascii_case(section)))?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|l| matches!(l, Line::Section(..)))
            .map(|p| start + 1 + p)
            .unwrap_or(self.lines.len());
        Some((start, end))
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        let (s, e) = self.section_range(section)?;
        self.lines[s + 1..e].iter().rev().find_map(|l| match l {
            Line::Pair { key: k, value, .. } if k.eq_ignore_ascii_case(key) => Some(value.clone()),
            _ => None,
        })
    }

    /// Set a single-valued key: replaces the last occurrence in place (keeps position), removes
    /// other duplicates, or appends to the section (creating it at the end of the file).
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let raw = format!("{key}={value}");
        match self.section_range(section) {
            Some((s, e)) => {
                let idx: Vec<usize> = (s + 1..e)
                    .filter(|&i| matches!(&self.lines[i], Line::Pair { key: k, .. } if k.eq_ignore_ascii_case(key)))
                    .collect();
                if let Some(&last) = idx.last() {
                    if let Line::Pair { value: v, .. } = &self.lines[last] {
                        if v == value && idx.len() == 1 {
                            return;
                        }
                    }
                    self.lines[last] = Line::Pair { raw, key: key.to_string(), value: value.to_string() };
                    for &i in idx.iter().rev().skip(1) {
                        self.lines.remove(i);
                    }
                } else {
                    // Insert after the last non-blank line of the section.
                    let mut at = e;
                    while at > s + 1 && matches!(&self.lines[at - 1], Line::Other(t) if t.trim().is_empty()) {
                        at -= 1;
                    }
                    self.lines.insert(at, Line::Pair { raw, key: key.to_string(), value: value.to_string() });
                }
            }
            None => {
                if !self.lines.is_empty() && !matches!(self.lines.last(), Some(Line::Other(t)) if t.trim().is_empty()) {
                    self.lines.push(Line::Other(String::new()));
                }
                self.lines.push(Line::Section(section.to_string(), format!("[{section}]")));
                self.lines.push(Line::Pair { raw, key: key.to_string(), value: value.to_string() });
            }
        }
    }

    /// Append a repeated key (e.g. ModHttp.ini `AllowedOrigin=`) unless that exact pair exists.
    pub fn add(&mut self, section: &str, key: &str, value: &str) {
        let exists = self.section_range(section).map(|(s, e)| {
            self.lines[s + 1..e].iter().any(|l| matches!(l, Line::Pair { key: k, value: v, .. } if k.eq_ignore_ascii_case(key) && v == value))
        });
        match exists {
            Some(true) => {}
            Some(false) => {
                let (s, e) = self.section_range(section).unwrap();
                let mut at = e;
                while at > s + 1 && matches!(&self.lines[at - 1], Line::Other(t) if t.trim().is_empty()) {
                    at -= 1;
                }
                self.lines.insert(at, Line::Pair { raw: format!("{key}={value}"), key: key.to_string(), value: value.to_string() });
            }
            None => self.set(section, key, value),
        }
    }

    /// Every section name in file order.
    pub fn sections(&self) -> Vec<String> {
        self.lines.iter().filter_map(|l| if let Line::Section(n, _) = l { Some(n.clone()) } else { None }).collect()
    }

    /// All `key=value` pairs of a section in file order (keys as written).
    pub fn pairs(&self, section: &str) -> Vec<(String, String)> {
        let Some((s, e)) = self.section_range(section) else { return vec![] };
        self.lines[s + 1..e]
            .iter()
            .filter_map(|l| if let Line::Pair { key, value, .. } = l { Some((key.clone(), value.clone())) } else { None })
            .collect()
    }

    /// Remove every occurrence of a key (including `+Key` / `-Key` array forms).
    pub fn remove(&mut self, section: &str, key: &str) -> bool {
        let Some((s, e)) = self.section_range(section) else { return false };
        let before = self.lines.len();
        let mut i = e;
        while i > s + 1 {
            i -= 1;
            if matches!(&self.lines[i], Line::Pair { key: k, .. } if base_key(k).eq_ignore_ascii_case(key)) {
                self.lines.remove(i);
            }
        }
        before != self.lines.len()
    }

    /// Remove one specific array entry whose value matches exactly.
    pub fn remove_value(&mut self, section: &str, key: &str, value: &str) -> bool {
        let Some((s, e)) = self.section_range(section) else { return false };
        if let Some(i) = (s + 1..e).rev().find(|&i| {
            matches!(&self.lines[i], Line::Pair { key: k, value: v, .. } if base_key(k).eq_ignore_ascii_case(key) && v == value)
        }) {
            self.lines.remove(i);
            return true;
        }
        false
    }
}

pub fn read_file(path: &Path) -> AppResult<String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            // UE writes UTF-16LE with BOM for non-ASCII content.
            if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
                let u16s: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                return Ok(String::from_utf16_lossy(&u16s));
            }
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}

/// Write atomically, keeping a rolling history copy of what was there before.
pub fn write_file(path: &Path, text: &str) -> AppResult<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    if let Ok(old) = std::fs::read(path) {
        if old == text.as_bytes() {
            return Ok(());
        }
        if let (Some(dir), Some(name)) = (path.parent(), path.file_name()) {
            let hist = dir.join("_history");
            if std::fs::create_dir_all(&hist).is_ok() {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let _ = std::fs::write(hist.join(format!("{}.{stamp}", name.to_string_lossy())), &old);
                prune_history(&hist, &name.to_string_lossy(), 30);
            }
        }
    }
    let tmp = path.with_extension("ini.tmp");
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn prune_history(dir: &Path, name: &str, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> = rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{name}."))).map(|e| e.path()).collect();
    files.sort();
    if files.len() > keep {
        for f in &files[..files.len() - keep] {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "; comment\r\n[/Script/Dominion.DedicatedServerSettings]\r\nOwnerId=abc\r\nServerName=Old\r\n+AdminList=(Id=\"x\")\r\n+AdminList=(Id=\"y\")\r\n\r\n[Other]\r\nA=1\r\n";

    #[test]
    fn round_trip_is_lossless() {
        let d = IniDoc::parse(SAMPLE);
        assert_eq!(d.to_text(), SAMPLE);
    }

    #[test]
    fn set_replaces_in_place_and_appends() {
        let mut d = IniDoc::parse(SAMPLE);
        d.set("/Script/Dominion.DedicatedServerSettings", "ServerName", "New Name");
        d.set("/Script/Dominion.DedicatedServerSettings", "WorldPassword", "pw");
        d.set("Fresh", "K", "V");
        let t = d.to_text();
        assert!(t.contains("ServerName=New Name\r\n+AdminList"));
        assert!(t.contains("+AdminList=(Id=\"y\")\r\nWorldPassword=pw\r\n\r\n[Other]"));
        assert!(t.ends_with("[Fresh]\r\nK=V\r\n"));
    }

    #[test]
    fn remove_handles_array_forms() {
        let mut d = IniDoc::parse(SAMPLE);
        assert!(d.remove_value("/Script/Dominion.DedicatedServerSettings", "AdminList", "(Id=\"x\")"));
        assert!(d.to_text().contains("+AdminList=(Id=\"y\")") && !d.to_text().contains("(Id=\"x\")"));
        assert!(d.remove("/Script/Dominion.DedicatedServerSettings", "AdminList"));
        assert!(!d.to_text().contains("AdminList"));
    }

}
