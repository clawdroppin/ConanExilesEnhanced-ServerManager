//! Discover the maps a server can run by scanning the installed game's `Content` folder for
//! `.umap` assets. This is more reliable than the Steam Web API (which doesn't list maps) and
//! automatically picks up new official maps and mod maps the moment they're installed.
//!
//! A map's *internal path* (what `Engine.ini` `ServerDefaultMap` expects) is `/Game/` + the asset's
//! path relative to `Content/`, minus the `.umap` extension:
//!   `Content/Maps/ConanSandbox/ConanSandbox.umap`            → `/Game/Maps/ConanSandbox/ConanSandbox`
//!   `Content/DLC_EXT/DLC_Siptah/Maps/DLC_Isle_of_Siptah.umap` → `/Game/DLC_EXT/DLC_Siptah/Maps/DLC_Isle_of_Siptah`

use serde::Serialize;
use std::path::Path;

/// One discoverable map.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MapInfo {
    /// Internal map path for `ServerDefaultMap` (e.g. `/Game/Maps/ConanSandbox/ConanSandbox`).
    pub path: String,
    /// Human-friendly name derived from the asset file name (e.g. `ConanSandbox`).
    pub label: String,
}

/// Recursively collect `.umap` files under `dir`, returning their internal map paths.
pub fn list_maps(content_dir: &Path) -> Vec<MapInfo> {
    let mut out: Vec<MapInfo> = Vec::new();
    walk(content_dir, content_dir, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(dir: &Path, content_root: &Path, out: &mut Vec<MapInfo>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            walk(&path, content_root, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".umap") {
            continue;
        }
        // Path relative to the Content root, with forward slashes (UE internal path convention).
        let Ok(rel) = path.strip_prefix(content_root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let rel = rel.trim_end_matches(".umap").to_string();
        if rel.is_empty() {
            continue;
        }
        let internal = format!("/Game/{rel}");
        let label = name.trim_end_matches(".umap").to_string();
        if !out.iter().any(|m| m.path == internal) {
            out.push(MapInfo { path: internal, label });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_internal_paths() {
        let dir = std::env::temp_dir().join(format!("cesm-maps-{}", uuid::Uuid::new_v4()));
        let a = dir.join("Maps").join("ConanSandbox");
        let b = dir.join("DLC_EXT").join("DLC_Siptah").join("Maps");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("ConanSandbox.umap"), b"").unwrap();
        std::fs::write(b.join("DLC_Isle_of_Siptah.umap"), b"").unwrap();
        std::fs::write(a.join("notamap.txt"), b"").unwrap();

        let maps = list_maps(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        let paths: Vec<&str> = maps.iter().map(|m| m.path.as_str()).collect();
        assert!(paths.contains(&"/Game/Maps/ConanSandbox/ConanSandbox"));
        assert!(paths.contains(&"/Game/DLC_EXT/DLC_Siptah/Maps/DLC_Isle_of_Siptah"));
        assert_eq!(maps.len(), 2, "only .umap assets should be listed: {paths:?}");
    }
}
