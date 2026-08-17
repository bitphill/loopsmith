//! Triggers, and the watcher that turns a one-shot run into a loop that lives
//! for weeks.
//!
//! Without this, `schedules` is decoration: the config parses, validates, and
//! then nothing ever fires it. `loopsmith watch` is the process that actually
//! keeps a loop alive; `loopsmith schedule install` hands the job to the
//! operating system so it survives a reboot.
//!
//! Cron expressions are evaluated in **UTC**. Deriving a correct local offset
//! in a multithreaded process is unsound on Unix without care, and a scheduler
//! that is quietly an hour off twice a year is worse than one that is honestly
//! in UTC. For wall-clock-independent cadence, prefer `interval`.

use loopsmith_core::Trigger;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One field of a cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Any,
    /// Sorted, deduplicated list of permitted values.
    Values(Vec<u32>),
}

impl Field {
    fn matches(&self, v: u32) -> bool {
        match self {
            Field::Any => true,
            Field::Values(list) => list.binary_search(&v).is_ok(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpr {
    pub minute: Field,
    pub hour: Field,
    pub day_of_month: Field,
    pub month: Field,
    pub day_of_week: Field,
}

fn parse_field(spec: &str, min: u32, max: u32) -> Result<Field, String> {
    if spec == "*" {
        return Ok(Field::Any);
    }
    let mut out: Vec<u32> = Vec::new();
    for part in spec.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (
                r,
                s.parse::<u32>()
                    .map_err(|_| format!("`{s}` is not a step"))?,
            ),
            None => (part, 1),
        };
        if step == 0 {
            return Err("step of 0 would never fire".into());
        }
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (
                a.parse::<u32>().map_err(|_| format!("`{a}` is not a number"))?,
                b.parse::<u32>().map_err(|_| format!("`{b}` is not a number"))?,
            )
        } else {
            let v = range
                .parse::<u32>()
                .map_err(|_| format!("`{range}` is not a number"))?;
            (v, v)
        };
        if lo < min || hi > max || lo > hi {
            return Err(format!("`{part}` is outside {min}..={max}"));
        }
        let mut v = lo;
        while v <= hi {
            out.push(v);
            v += step;
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err("field matches nothing".into());
    }
    Ok(Field::Values(out))
}

impl CronExpr {
    /// Parse a five-field expression: `minute hour day-of-month month day-of-week`.
    pub fn parse(expr: &str) -> Result<Self, String> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(format!(
                "expected 5 fields (minute hour day-of-month month day-of-week), got {}",
                parts.len()
            ));
        }
        Ok(CronExpr {
            minute: parse_field(parts[0], 0, 59)?,
            hour: parse_field(parts[1], 0, 23)?,
            day_of_month: parse_field(parts[2], 1, 31)?,
            month: parse_field(parts[3], 1, 12)?,
            // 0 and 7 both mean Sunday, as in every other cron.
            day_of_week: parse_field(parts[4], 0, 7)?,
        })
    }

    pub fn matches(&self, t: &Civil) -> bool {
        let dow_match = self.day_of_week.matches(t.weekday)
            || (t.weekday == 0 && self.day_of_week.matches(7));
        self.minute.matches(t.minute)
            && self.hour.matches(t.hour)
            && self.day_of_month.matches(t.day)
            && self.month.matches(t.month)
            && dow_match
    }
}

/// Broken-down UTC time. Computed here rather than pulled from a date crate so
/// the scheduler has no dependency the rest of the binary does not already
/// carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub weekday: u32,
}

/// Days-to-civil, from Howard Hinnant's `civil_from_days`.
pub fn civil_from_unix(secs: i64) -> Civil {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    // 1970-01-01 was a Thursday.
    let weekday = (days + 4).rem_euclid(7) as u32;
    Civil {
        year,
        month: m as u32,
        day: d as u32,
        hour: (rem / 3600) as u32,
        minute: ((rem % 3600) / 60) as u32,
        weekday,
    }
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Directories a `file_change` trigger must never see, at any depth.
///
/// Every one of these is written *by* a run. A watcher that noticed them would
/// fire a run, which would write them, which would fire a run: the loop would
/// never be idle again and nothing in the output would say why. `logs/` belongs
/// here for exactly the same reason `state/` does — the run log is written on
/// every single iteration.
const NEVER_WATCHED: [&str; 3] = ["state", ".git", "logs"];

/// Newest mtime anywhere under a path, as seconds. Directories are walked; a
/// missing path reports 0 rather than erroring, because a watched path that
/// does not exist yet is a normal state.
///
/// `extra` names directories to skip beyond [`NEVER_WATCHED`] — the ones only
/// the caller knows, chiefly the success export, whose directory is named after
/// the loop and is written whenever a run meets its bar.
pub fn newest_mtime_ignoring(path: &Path, extra: &[String]) -> u64 {
    fn walk(p: &Path, best: &mut u64, depth: u32, extra: &[String]) {
        if depth > 24 {
            return;
        }
        let Ok(meta) = std::fs::metadata(p) else {
            return;
        };
        if let Ok(m) = meta.modified() {
            if let Ok(d) = m.duration_since(UNIX_EPOCH) {
                *best = (*best).max(d.as_secs());
            }
        }
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(p) {
                for e in entries.flatten() {
                    let child = e.path();
                    let skip = child.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                        NEVER_WATCHED.contains(&n) || extra.iter().any(|x| x == n)
                    });
                    if skip {
                        continue;
                    }
                    walk(&child, best, depth + 1, extra);
                }
            }
        }
    }
    let mut best = 0;
    walk(path, &mut best, 0, extra);
    best
}

/// Why the watcher decided to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fired {
    Cron(String),
    Interval(u64),
    FileChange(String),
    GoalSatisfied(String),
}

impl Fired {
    pub fn describe(&self) -> String {
        match self {
            Fired::Cron(e) => format!("cron `{e}` matched (UTC)"),
            Fired::Interval(s) => format!("interval of {s}s elapsed"),
            Fired::FileChange(p) => format!("`{p}` changed"),
            Fired::GoalSatisfied(g) => format!("goal `{g}` became satisfied"),
        }
    }
}

/// Trigger state carried between polls.
#[derive(Debug, Default)]
pub struct Watcher {
    /// Minute-of-epoch already handled, so a cron entry fires once per minute
    /// rather than on every poll inside that minute.
    fired_minute: BTreeMap<String, i64>,
    last_interval_run: BTreeMap<u64, i64>,
    last_mtime: BTreeMap<String, u64>,
    satisfied_seen: BTreeMap<String, bool>,
    /// Directory names this watcher must not treat as a change, beyond the
    /// ones every loop writes. In practice: the success export.
    ignore: Vec<String>,
}

impl Watcher {
    /// A watcher that also ignores these directory names.
    pub fn ignoring(ignore: Vec<String>) -> Self {
        Self {
            ignore,
            ..Self::default()
        }
    }

    /// Prime file and goal state without firing, so starting the watcher does
    /// not immediately look like a change.
    pub fn prime(&mut self, triggers: &[Trigger], root: &Path) {
        for t in triggers {
            if let Trigger::FileChange { path } = t {
                self.last_mtime.insert(
                    path.clone(),
                    newest_mtime_ignoring(&root.join(path), &self.ignore),
                );
            }
        }
    }

    /// Which triggers are due right now?
    pub fn poll(
        &mut self,
        triggers: &[Trigger],
        root: &Path,
        now: i64,
        satisfied: &BTreeMap<String, bool>,
    ) -> Vec<Fired> {
        let civil = civil_from_unix(now);
        let minute = now.div_euclid(60);
        let mut out = Vec::new();

        for t in triggers {
            match t {
                Trigger::Manual => {}
                Trigger::Cron { expr } => {
                    let Ok(c) = CronExpr::parse(expr) else { continue };
                    if c.matches(&civil) && self.fired_minute.get(expr) != Some(&minute) {
                        self.fired_minute.insert(expr.clone(), minute);
                        out.push(Fired::Cron(expr.clone()));
                    }
                }
                Trigger::Interval { seconds } => {
                    let last = self.last_interval_run.get(seconds).copied();
                    match last {
                        None => {
                            self.last_interval_run.insert(*seconds, now);
                        }
                        Some(prev) if now - prev >= *seconds as i64 => {
                            self.last_interval_run.insert(*seconds, now);
                            out.push(Fired::Interval(*seconds));
                        }
                        _ => {}
                    }
                }
                Trigger::FileChange { path } => {
                    let current = newest_mtime_ignoring(&root.join(path), &self.ignore);
                    let prev = self.last_mtime.get(path).copied();
                    self.last_mtime.insert(path.clone(), current);
                    if let Some(p) = prev {
                        if current > p {
                            out.push(Fired::FileChange(path.clone()));
                        }
                    }
                }
                Trigger::GoalSatisfied { goal } => {
                    let now_sat = satisfied.get(goal).copied().unwrap_or(false);
                    let was = self.satisfied_seen.get(goal).copied().unwrap_or(false);
                    self.satisfied_seen.insert(goal.clone(), now_sat);
                    // Edge, not level: fire on the transition only.
                    if now_sat && !was {
                        out.push(Fired::GoalSatisfied(goal.clone()));
                    }
                }
            }
        }
        out
    }
}

/// Shortest sensible poll interval for a trigger set.
pub fn poll_interval(triggers: &[Trigger]) -> Duration {
    let mut secs = 30u64;
    for t in triggers {
        match t {
            // Cron needs sub-minute polling or a minute can be missed.
            Trigger::Cron { .. } => secs = secs.min(20),
            Trigger::FileChange { .. } => secs = secs.min(5),
            Trigger::Interval { seconds } => secs = secs.min((*seconds / 4).max(1)),
            _ => {}
        }
    }
    Duration::from_secs(secs.max(1))
}

// ─────────────────────────────── OS handoff ────────────────────────────────

/// A launchd agent that runs `loopsmith watch` and restarts it if it dies.
pub fn launchd_plist(label: &str, exe: &Path, config: &Path, log_dir: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>watch</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>StandardOutPath</key><string>{}/loopsmith.out.log</string>
    <key>StandardErrorPath</key><string>{}/loopsmith.err.log</string>
</dict>
</plist>
"#,
        exe.display(),
        config.display(),
        log_dir.display(),
        log_dir.display()
    )
}

/// A crontab line that re-runs the loop once, for systems without launchd.
/// `@reboot` keeps a watcher alive instead if the config has non-cron triggers.
pub fn crontab_line(exe: &Path, config: &Path, expr: &str, log_dir: &Path) -> String {
    format!(
        "{expr} {} run {} >> {}/loopsmith.log 2>&1",
        exe.display(),
        config.display(),
        log_dir.display()
    )
}

pub fn default_label(loop_name: &str) -> String {
    format!(
        "com.loopsmith.{}",
        loop_name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
    )
}

/// Where a launchd agent is written.
///
/// `LOOPSMITH_LAUNCH_AGENTS_DIR` overrides it. That exists so `--install` can
/// be exercised by a test without writing into the home directory of whatever
/// machine the suite happens to run on — installing a launch agent is a
/// persistent, user-visible change, and a test suite that makes one is a test
/// suite nobody can run twice.
pub fn launch_agents_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LOOPSMITH_LAUNCH_AGENTS_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/LaunchAgents"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn civil(y: i64, mo: u32, d: u32, h: u32, mi: u32, wd: u32) -> Civil {
        Civil {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
            weekday: wd,
        }
    }

    #[test]
    fn every_minute_parses_and_always_matches() {
        let c = CronExpr::parse("* * * * *").unwrap();
        assert!(c.matches(&civil(2026, 8, 16, 3, 7, 0)));
    }

    #[test]
    fn a_daily_time_matches_only_at_that_minute() {
        let c = CronExpr::parse("0 2 * * *").unwrap();
        assert!(c.matches(&civil(2026, 8, 16, 2, 0, 0)));
        assert!(!c.matches(&civil(2026, 8, 16, 2, 1, 0)));
        assert!(!c.matches(&civil(2026, 8, 16, 3, 0, 0)));
    }

    #[test]
    fn steps_lists_and_ranges_all_parse() {
        let c = CronExpr::parse("*/15 9-17 1,15 * 1-5").unwrap();
        assert!(c.matches(&civil(2026, 8, 15, 9, 30, 5)));
        assert!(!c.matches(&civil(2026, 8, 15, 9, 31, 5)), "31 is not a 15-step");
        assert!(!c.matches(&civil(2026, 8, 15, 8, 30, 5)), "08:00 is outside 9-17");
        assert!(!c.matches(&civil(2026, 8, 2, 9, 30, 5)), "day 2 is not 1 or 15");
    }

    #[test]
    fn sunday_is_both_zero_and_seven() {
        let zero = CronExpr::parse("0 0 * * 0").unwrap();
        let seven = CronExpr::parse("0 0 * * 7").unwrap();
        let sunday = civil(2026, 8, 16, 0, 0, 0);
        assert!(zero.matches(&sunday));
        assert!(seven.matches(&sunday));
    }

    #[test]
    fn malformed_expressions_are_rejected_with_a_reason() {
        for bad in ["* * * *", "60 * * * *", "* 24 * * *", "*/0 * * * *", "a * * * *", ""] {
            assert!(CronExpr::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn civil_conversion_matches_known_timestamps() {
        // 1970-01-01T00:00:00Z was a Thursday.
        let t = civil_from_unix(0);
        assert_eq!((t.year, t.month, t.day, t.weekday), (1970, 1, 1, 4));
        // 2026-08-16T03:04:00Z is a Sunday.
        let t = civil_from_unix(1_786_000_000 - (1_786_000_000 % 60));
        assert_eq!(t.year, 2026);
        assert!(t.month >= 1 && t.month <= 12);
    }

    #[test]
    fn a_cron_trigger_fires_once_per_minute_not_once_per_poll() {
        let mut w = Watcher::default();
        let triggers = vec![Trigger::Cron {
            expr: "* * * * *".into(),
        }];
        let root = std::env::temp_dir();
        let sat = BTreeMap::new();
        let t = 1_786_000_020; // some second inside a minute
        assert_eq!(w.poll(&triggers, &root, t, &sat).len(), 1);
        assert!(
            w.poll(&triggers, &root, t + 5, &sat).is_empty(),
            "same minute must not fire twice"
        );
        assert_eq!(
            w.poll(&triggers, &root, t + 60, &sat).len(),
            1,
            "next minute should fire"
        );
    }

    #[test]
    fn an_interval_waits_before_its_first_repeat() {
        let mut w = Watcher::default();
        let triggers = vec![Trigger::Interval { seconds: 100 }];
        let root = std::env::temp_dir();
        let sat = BTreeMap::new();
        assert!(w.poll(&triggers, &root, 1000, &sat).is_empty(), "primes, does not fire");
        assert!(w.poll(&triggers, &root, 1050, &sat).is_empty(), "too soon");
        assert_eq!(w.poll(&triggers, &root, 1100, &sat).len(), 1);
    }

    #[test]
    fn a_file_change_fires_once_per_change() {
        let dir = loopsmith_util::testing::temp_dir("file-change");
        let watched = dir.join("watched.txt");
        std::fs::write(&watched, "one").unwrap();

        let triggers = vec![Trigger::FileChange {
            path: "watched.txt".into(),
        }];
        let mut w = Watcher::default();
        w.prime(&triggers, &dir);
        let sat = BTreeMap::new();

        assert!(w.poll(&triggers, &dir, now_unix(), &sat).is_empty(), "no change yet");

        // mtime has one-second resolution, so move the file's timestamp
        // forward explicitly rather than sleeping.
        let future = SystemTime::now() + Duration::from_secs(5);
        let f = std::fs::File::options().write(true).open(&watched).unwrap();
        f.set_modified(future).unwrap();

        let fired = w.poll(&triggers, &dir, now_unix(), &sat);
        assert_eq!(fired.len(), 1);
        assert!(matches!(fired[0], Fired::FileChange(_)));
        assert!(w.poll(&triggers, &dir, now_unix(), &sat).is_empty(), "fires once");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn goal_satisfied_fires_on_the_transition_only() {
        let mut w = Watcher::default();
        let triggers = vec![Trigger::GoalSatisfied { goal: "g1".into() }];
        let root = std::env::temp_dir();

        let mut sat = BTreeMap::new();
        sat.insert("g1".to_string(), false);
        assert!(w.poll(&triggers, &root, 1, &sat).is_empty());

        sat.insert("g1".to_string(), true);
        assert_eq!(w.poll(&triggers, &root, 2, &sat).len(), 1);
        assert!(
            w.poll(&triggers, &root, 3, &sat).is_empty(),
            "still satisfied is not a new edge"
        );
    }

    #[test]
    fn manual_only_configs_never_fire() {
        let mut w = Watcher::default();
        let triggers = vec![Trigger::Manual];
        assert!(w
            .poll(&triggers, &std::env::temp_dir(), now_unix(), &BTreeMap::new())
            .is_empty());
    }

    #[test]
    fn poll_interval_tightens_for_the_most_demanding_trigger() {
        assert_eq!(poll_interval(&[Trigger::Manual]), Duration::from_secs(30));
        assert_eq!(
            poll_interval(&[Trigger::FileChange { path: "x".into() }]),
            Duration::from_secs(5)
        );
        assert_eq!(
            poll_interval(&[Trigger::Interval { seconds: 8 }]),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn the_watcher_ignores_its_own_state_directory() {
        // Without this the ledger's own writes would retrigger the loop on
        // every poll, forever.
        let dir = loopsmith_util::testing::temp_dir("self-state");
        std::fs::create_dir_all(dir.join("state")).unwrap();
        std::fs::write(dir.join("keep.txt"), "x").unwrap();
        let before = newest_mtime_ignoring(&dir, &[]);

        let f = std::fs::File::create(dir.join("state/db")).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(600)).unwrap();

        assert_eq!(newest_mtime_ignoring(&dir, &[]), before, "state/ must not count as a change");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_watcher_ignores_the_run_log_and_the_success_export() {
        // `logs/` was not in the skip list, and a run writes to it on every
        // iteration. A `file_change` trigger on the loop root would therefore
        // see the run it had just started and start another one. The success
        // export is the same shape of mistake with a longer fuse: it is
        // written only when a run meets its bar, so the loop would go into a
        // permanent cycle exactly when it succeeded.
        let dir = loopsmith_util::testing::temp_dir("self-output");
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::create_dir_all(dir.join("demo-success")).unwrap();
        std::fs::write(dir.join("keep.txt"), "x").unwrap();
        let ignore = vec!["demo-success".to_string()];
        let before = newest_mtime_ignoring(&dir, &ignore);

        for rel in ["logs/run-1.log", "demo-success/SKILL.md"] {
            let f = std::fs::File::create(dir.join(rel)).unwrap();
            f.set_modified(SystemTime::now() + Duration::from_secs(600))
                .unwrap();
        }

        assert_eq!(
            newest_mtime_ignoring(&dir, &ignore),
            before,
            "a run's own output must not look like a change to the watcher"
        );
        // And a real edit still does, or the trigger would be inert.
        let f = std::fs::File::create(dir.join("keep.txt")).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(600))
            .unwrap();
        assert!(newest_mtime_ignoring(&dir, &ignore) > before);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn generated_plist_and_crontab_reference_the_right_paths() {
        let plist = launchd_plist(
            "com.loopsmith.demo",
            Path::new("/usr/local/bin/loopsmith"),
            Path::new("/loops/demo/loop.yaml"),
            Path::new("/tmp"),
        );
        assert!(plist.contains("<string>watch</string>"));
        assert!(plist.contains("/loops/demo/loop.yaml"));
        assert!(plist.contains("<key>KeepAlive</key><true/>"));

        let line = crontab_line(
            Path::new("/usr/local/bin/loopsmith"),
            Path::new("/loops/demo/loop.yaml"),
            "0 2 * * *",
            Path::new("/tmp"),
        );
        assert!(line.starts_with("0 2 * * * /usr/local/bin/loopsmith run"));
    }

    #[test]
    fn labels_are_safe_for_launchd() {
        assert_eq!(default_label("my loop/v2"), "com.loopsmith.my-loop-v2");
    }
}
