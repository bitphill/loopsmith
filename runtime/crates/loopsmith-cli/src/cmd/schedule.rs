//! `loopsmith schedule` — hand the schedule to the operating system.

use super::config_dir;
use crate::schedule;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn execute(config: &Path, install: bool) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("loopsmith"));
    let abs = std::fs::canonicalize(config).unwrap_or_else(|_| config.to_path_buf());
    let root = config_dir(config);
    // Not `state/`: that directory is sled's, and the watcher ignores
    // everything inside it — including, until this moved, the operating
    // system's own record of why the loop failed to start.
    let logs = root.join("logs");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let label = schedule::default_label(&cfg.name);

    if cfg!(target_os = "macos") {
        let plist = schedule::launchd_plist(&label, &exe, &abs, &logs);
        let dest = schedule::launch_agents_dir()
            .ok_or("HOME is not set, cannot locate LaunchAgents")?
            .join(format!("{label}.plist"));
        if install {
            std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&dest, &plist).map_err(|e| e.to_string())?;
            println!("wrote {}", dest.display());
            // Loading it is a persistent, user-visible change to their machine,
            // so it stays their call.
            println!("\nEnable it with:\n  launchctl load -w {}", dest.display());
        } else {
            println!("{plist}");
            println!(
                "# write it with: loopsmith schedule {} --install",
                config.display()
            );
        }
    } else {
        let expr = cfg
            .schedules
            .iter()
            .find_map(|t| match t {
                loopsmith_core::Trigger::Cron { expr } => Some(expr.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "@reboot".into());
        println!("{}", schedule::crontab_line(&exe, &abs, &expr, &logs));
        println!("# add it with: crontab -e");
    }
    Ok(ExitCode::SUCCESS)
}
