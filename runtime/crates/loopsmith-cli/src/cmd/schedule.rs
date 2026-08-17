//! `loopsmith schedule` — hand the schedule to the operating system.
//!
//! Which scheduler to use is decided by what is **installed**, not by what the
//! operating system is famous for. A `cfg!(target_os = "macos")` says the build
//! target had launchd; it does not say this machine has `launchctl` on `PATH`,
//! and a container built `FROM debian` has neither `crontab` nor `systemctl`
//! unless someone put them there. Printing a crontab line to a host with no
//! cron is a instruction that silently does nothing.

use super::config_dir;
use crate::schedule;
use loopsmith_util::platform::Platform;
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

    let platform = Platform::detect();
    match platform.scheduler() {
        Some("launchctl") => launchd(&label, &exe, &abs, &logs, config, install),
        Some("schtasks") => {
            schtasks(&label, &exe, &abs);
            Ok(ExitCode::SUCCESS)
        }
        Some(_) => {
            crontab(&cfg, &exe, &abs, &logs);
            Ok(ExitCode::SUCCESS)
        }
        None => Err(format!(
            "no scheduler on this machine (looked for {}). \n\
             The loop can still be driven by `loopsmith watch`, which needs a process \n\
             supervisor of some kind — a systemd unit, a container restart policy, or a \n\
             terminal you leave open.",
            preferred_names(&platform).join(", ")
        )),
    }
}

/// The candidates that *were* looked for, so the error names the right tool.
///
/// Reusing the same list the probe used, rather than restating it: the version
/// that restated it told a Windows user to install `crontab`.
fn preferred_names(p: &Platform) -> Vec<&'static str> {
    loopsmith_util::platform::preferred_schedulers(p.os).to_vec()
}

fn launchd(
    label: &str,
    exe: &Path,
    abs: &Path,
    logs: &Path,
    config: &Path,
    install: bool,
) -> Result<ExitCode, String> {
    let plist = schedule::launchd_plist(label, exe, abs, logs);
    if !install {
        println!("{plist}");
        println!(
            "# write it with: loopsmith schedule {} --install",
            config.display()
        );
        return Ok(ExitCode::SUCCESS);
    }
    let dest = schedule::launch_agents_dir()
        .ok_or("HOME is not set, cannot locate LaunchAgents")?
        .join(format!("{label}.plist"));
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&dest, &plist).map_err(|e| e.to_string())?;
    println!("wrote {}", dest.display());
    // Loading it is a persistent, user-visible change to their machine, so it
    // stays their call.
    println!("\nEnable it with:\n  launchctl load -w {}", dest.display());
    Ok(ExitCode::SUCCESS)
}

/// Windows Task Scheduler has no crontab-shaped file to append to — the
/// schedule is created by running a command — so this prints the command rather
/// than writing anything. Creating a scheduled task is a persistent change to
/// the user's machine, the same reason `launchctl load` is left to them.
fn schtasks(label: &str, exe: &Path, abs: &Path) {
    println!("{}", crate::schedule::schtasks_command(label, exe, abs));
    println!();
    println!("# Run that in an elevated-or-not shell to register the task.");
    println!("# It keeps `loopsmith watch` alive; the watcher evaluates the triggers in");
    println!("# section G itself, so Task Scheduler only has to restart one process.");
    println!("# Remove it later with: schtasks /Delete /TN \"{label}\" /F");
}

fn crontab(cfg: &loopsmith_core::LoopConfig, exe: &Path, abs: &Path, logs: &Path) {
    let expr = cfg
        .schedules
        .iter()
        .find_map(|t| match t {
            loopsmith_core::Trigger::Cron { expr } => Some(expr.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "@reboot".into());
    println!("{}", schedule::crontab_line(exe, abs, &expr, logs));
    println!("# add it with: crontab -e");
    println!("# cron is evaluated in UTC by loopsmith but in local time by cron itself;");
    println!("# for a cadence that does not care, prefer an `interval` trigger.");
}
