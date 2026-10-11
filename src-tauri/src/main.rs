// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Helper mode: `--send-ctrl-break <pid>` attaches to a server's console and asks it to shut down
    // gracefully. Runs as a short-lived child so the manager itself never touches foreign consoles.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--send-ctrl-break" {
        let code = args[2].parse::<u32>().map(cesm_lib::send_ctrl_break).unwrap_or(2);
        std::process::exit(code);
    }
    cesm_lib::run()
}
