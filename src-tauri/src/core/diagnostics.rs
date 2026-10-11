//! Health checks for one server, each with an optional one-click fix the UI can trigger.

use super::{ini, mods, network, proc, steamcmd};
use crate::models::*;
use crate::state::AppState;

fn check(id: &str, title: &str, level: &str, detail: impl Into<String>, fix: &str) -> DiagCheck {
    DiagCheck { id: id.into(), title: title.into(), level: level.into(), detail: detail.into(), fix: fix.into() }
}

/// 0–100 summary of a check list (used for the health badge).
pub fn score(checks: &[DiagCheck]) -> u32 {
    let mut s: i32 = 100;
    for c in checks {
        s -= match c.level.as_str() {
            "error" => 25,
            "warn" => 8,
            _ => 0,
        };
    }
    s.clamp(0, 100) as u32
}

pub async fn run(state: &AppState, inst: &ServerInstance) -> Vec<DiagCheck> {
    let mut out = vec![];
    let installed = inst.is_installed();

    // Files
    if installed {
        out.push(check("install", "Server files", "ok", inst.server_exe().display().to_string(), ""));
        match steamcmd::manifest_state_flags(&inst.install_dir) {
            Some(4) | None => {}
            Some(f) => out.push(check("manifest", "Interrupted update", "warn", format!("Steam reports state flags {f} – a download was interrupted. Validate the files."), "validate")),
        }
    } else {
        out.push(check("install", "Server files", "error", "The dedicated server isn't installed in this folder yet.", "install"));
    }

    // Branch
    if inst.is_legacy() {
        out.push(check("branch", "Game version", "warn", "This server runs the Legacy (UE4) branch. Players on Conan Exiles Enhanced can't join it.", ""));
    } else if inst.branch.trim().is_empty() || inst.branch.eq_ignore_ascii_case("public") {
        out.push(check("branch", "Game version", "ok", "Conan Exiles Enhanced (public branch).", ""));
    } else {
        out.push(check("branch", "Game version", "info", format!("Beta branch “{}”.", inst.branch), ""));
    }

    // Identity / security
    if inst.identity.admin_password.trim().is_empty() {
        out.push(check("admin", "Admin password", "warn", "No admin password – nobody can use admin commands in-game.", "settings"));
    } else if inst.identity.admin_password.len() < 8 {
        out.push(check("admin", "Admin password", "warn", "The admin password is short – anyone who guesses it gets full admin rights in-game.", "settings"));
    } else {
        out.push(check("admin", "Admin password", "ok", "Set.", ""));
    }

    // RCON
    if !inst.rcon.enabled {
        out.push(check("rcon", "RCON", "warn", "RCON is off – stops can't save cleanly, settings can't be changed live and players can't be listed.", "rcon"));
    } else {
        let running = super::server::running_pids(inst).1.is_some();
        let ok = state.with_runtime(&inst.id, |r| r.rcon_ok).await;
        if running && !ok {
            match state.rcon_exec(inst, "listplayers").await {
                Ok(_) => out.push(check("rcon", "RCON", "ok", format!("Connected on TCP {}.", inst.rcon.port), "")),
                Err(e) => out.push(check("rcon", "RCON", "error", format!("The server doesn't answer RCON: {e}. Restart it so the manager's RCON settings are applied."), "rcon")),
            }
        } else {
            out.push(check("rcon", "RCON", "ok", format!("Managed on TCP {} (never opened in the firewall).", inst.rcon.port), ""));
        }
    }

    // Engine.ini leftovers from UE4 that stop players from joining (Funcom troubleshooting).
    let engine = ini::IniDoc::parse(&ini::read_file(&inst.engine_ini()).unwrap_or_default());
    if engine.get("OnlineSubsystem", "BuildIdOverride").is_some() || engine.get("OnlineSubsystem", "bUseBuildIdOverride").is_some() {
        out.push(check("buildoverride", "Stale build override", "error", "Engine.ini still has bUseBuildIdOverride/BuildIdOverride from the UE4 server – Enhanced clients get rejected. The manager removes it on the next start.", "buildOverride"));
    }

    // Ports
    let instances = state.instances.read().await.clone();
    let mine = inst.udp_ports();
    let clash: Vec<String> = instances
        .iter()
        .filter(|o| o.id != inst.id)
        .filter(|o| o.udp_ports().iter().any(|p| mine.contains(p)) || o.rcon.port == inst.rcon.port)
        .map(|o| o.name.clone())
        .collect();
    if !clash.is_empty() {
        out.push(check("ports", "Port conflict", "error", format!("Ports shared with: {}. Only one can run at a time.", clash.join(", ")), "ports"));
    }
    let (pids, _) = super::server::running_pids(inst);
    let owners = proc::udp_port_owners().await;
    let busy: Vec<(u16, u32)> = mine.iter().filter_map(|p| owners.get(p).filter(|o| !pids.contains(o)).map(|o| (*p, *o))).collect();
    if let Some((port, pid)) = busy.first() {
        let mut sys = sysinfo::System::new();
        let name = proc::process_name(&mut sys, *pid).unwrap_or_default();
        out.push(check("portbusy", "Port in use", "error", format!("UDP {port} is held by {name} (PID {pid}). Pick other ports."), "ports"));
    } else if clash.is_empty() {
        out.push(check(
            "ports",
            "Ports",
            "ok",
            format!("Game UDP {}+{}, query UDP {} are free{}.", inst.launch.port, inst.pinger_port(), inst.launch.query_port, if pids.is_empty() { "" } else { " (used by this server)" }),
            "",
        ));
    }
    if !inst.launch.multihome.trim().is_empty() && inst.launch.multihome_http.trim().is_empty() {
        out.push(check("multihome", "MULTIHOME", "info", "MULTIHOMEHTTP is set to the same IP automatically so the server can register with the browser.", ""));
    }

    // Firewall
    let fw = network::firewall_state(inst).await;
    if !fw.blocked_by.is_empty() {
        out.push(check(
            "firewall",
            "Windows Firewall is blocking the server",
            "error",
            format!("Inbound Block rule(s) “{}” exist for the server – usually from dismissing Windows' “allow access?” prompt. Block beats Allow.", fw.blocked_by.join("”, “")),
            "firewall",
        ));
    } else if fw.ports_ok || fw.program_rule {
        out.push(check("firewall", "Windows Firewall", "ok", "An inbound rule allows the server.", ""));
    } else {
        out.push(check("firewall", "Windows Firewall", "warn", "No inbound rule for this server – players outside your PC may not reach it. Also forward the UDP ports on your router.", "firewall"));
    }

    // Mods
    let cache = state.workshop_dir().await;
    let enabled: Vec<&ModEntry> = inst.mods.iter().filter(|m| m.enabled).collect();
    if !enabled.is_empty() {
        let missing: Vec<String> = enabled.iter().filter(|m| mods::resolve_pak(&cache, m).is_none()).map(|m| if m.name.is_empty() { m.key() } else { m.name.clone() }).collect();
        let devkit = state.with_runtime(&inst.id, |r| r.devkit_revision).await.unwrap_or(KNOWN_DEVKIT_REVISION);
        let wrong_kit: Vec<String> = enabled
            .iter()
            .filter_map(|m| {
                let p = mods::resolve_pak(&cache, m)?;
                let i = mods::read_modinfo(&p)?;
                (i.devkit_revision.is_some() && i.devkit_revision != Some(devkit)).then(|| i.name)
            })
            .collect();
        if !missing.is_empty() {
            out.push(check("mods", "Mods", "warn", format!("Not downloaded yet: {} (fetched on the next start).", missing.join(", ")), "mods"));
        } else if !wrong_kit.is_empty() && !inst.is_legacy() {
            out.push(check("mods", "Mods", "error", format!("Built for a different devkit than this server ({devkit}): {}. They won't load.", wrong_kit.join(", ")), "mods"));
        } else {
            out.push(check("mods", "Mods", "ok", format!("{} enabled mod(s) on disk.", enabled.len()), ""));
        }
    }

    // Memory (Funcom/hosts: ~8 GB minimum, 16 GB+ for larger servers; mods add more)
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_gb = sys.total_memory() as f64 / 1_073_741_824.0;
    let need = 8.0 + (inst.launch.max_players as f64 / 10.0) + enabled.len() as f64 * 0.25;
    let excluded = !inst.regions.excluded.is_empty();
    if total_gb < need {
        out.push(check(
            "ram",
            "Memory",
            "warn",
            format!("About {need:.0} GB recommended for {} players and {} mods; this PC has {total_gb:.1} GB.{}", inst.launch.max_players, enabled.len(), if excluded { "" } else { " Excluding a map region (Server Setup → Map) saves RAM." }),
            "",
        ));
    } else {
        out.push(check("ram", "Memory", "ok", format!("{total_gb:.1} GB installed – about {need:.0} GB recommended."), ""));
    }

    // Disk
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let dir = proc::norm(&inst.install_dir);
    if let Some(d) = disks.iter().filter(|d| dir.starts_with(&proc::norm(d.mount_point()))).max_by_key(|d| d.mount_point().as_os_str().len()) {
        let free = d.available_space() as f64 / 1_073_741_824.0;
        let lvl = if free < 5.0 { "error" } else if free < 15.0 { "warn" } else { "ok" };
        out.push(check("disk", "Disk space", lvl, format!("{free:.1} GB free on {}.", d.mount_point().display()), ""));
    }

    // Runtime libraries
    let sys32 = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("System32");
    let redist = ["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll"].iter().all(|f| sys32.join(f).exists());
    if redist {
        out.push(check("vcredist", "Visual C++ runtime", "ok", "Installed.", ""));
    } else {
        out.push(check("vcredist", "Visual C++ runtime", "error", "The Microsoft Visual C++ 2015-2022 x64 runtime is missing – the server can't start without it.", "prereqs"));
    }

    // Updates
    let (installed_b, latest_b) = state.with_runtime(&inst.id, |r| (r.installed_build.clone(), r.latest_build.clone())).await;
    if let (Some(i), Some(l)) = (&installed_b, &latest_b) {
        if i != l {
            out.push(check("update", "Server version", "warn", format!("Build {i} is installed but {l} is live – players on the new game version can't join until you update."), "update"));
        } else {
            out.push(check("update", "Server version", "ok", format!("Up to date (build {i})."), ""));
        }
    }

    // Last log
    if let Ok(text) = std::fs::read(inst.log_path()).map(|b| String::from_utf8_lossy(&b).into_owned()) {
        let bad: Vec<&str> = text.lines().filter(|l| l.contains("Fatal error") || l.contains("Critical error") || l.contains("Assertion failed")).take(3).collect();
        if !bad.is_empty() {
            out.push(check("log", "Errors in the last log", "error", bad.join("\n"), "log"));
        }
        if text.contains("Autologin attempt failed, unable to register server") && text.contains("Started SourceServerQueries") {
            out.push(check(
                "register",
                "Server browser registration",
                "info",
                "The log reports “unable to register server” with Funcom Live Services. Verified on a fresh install: this appears on every start and the server still answers Steam queries.",
                "",
            ));
        }
    }
    out
}

/// Install the Visual C++ runtime – prefer the copy shipped with the server (_CommonRedist).
pub fn prereq_installer(inst: &ServerInstance) -> Option<std::path::PathBuf> {
    let p = inst.install_dir.join("_CommonRedist").join("vcredist").join("2022").join("VC_redist.x64.exe");
    p.exists().then_some(p)
}
