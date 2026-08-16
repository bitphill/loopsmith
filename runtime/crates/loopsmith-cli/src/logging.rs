//! The run log: a plain-text record of what happened, iteration by iteration.
//!
//! The sled ledger is the durable, queryable record, but it is a database — you
//! cannot `tail -f` it, and after an unattended run finishes at 4am the first
//! thing anyone wants is a file to scroll. This writes that file, from the same
//! single choke point the ledger is written through, so the two cannot disagree
//! about what happened.
//!
//! It lives in `<loop>/logs/`, deliberately **not** in `<loop>/state/`: sled
//! owns that directory, and the watcher ignores everything inside it.
//!
//! Failures here are swallowed. A full disk is a reason to lose the log, not a
//! reason to kill a run that is otherwise doing useful work.

use crate::schedule::civil_from_unix;
use loopsmith_memory::{LedgerEntry, LedgerKind, Store};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `2026-08-16T12:34:56Z`, from the same hand-rolled civil-date arithmetic the
/// cron matcher uses. UTC, for the reason documented on the scheduler: deriving
/// a local offset is unsound in a multithreaded process on Unix.
pub fn format_utc(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let c = civil_from_unix(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        c.year,
        c.month,
        c.day,
        c.hour,
        c.minute,
        secs.rem_euclid(60)
    )
}

/// An open run log, or a no-op if one could not be created.
pub struct RunLog {
    file: Option<Mutex<File>>,
    path: Option<PathBuf>,
    verbose: bool,
}

impl RunLog {
    /// Open `<root>/logs/run-<run_id>.log`, creating the directory if needed.
    pub fn open(root: &Path, run_id: &str, verbose: bool) -> Self {
        let dir = root.join("logs");
        let path = dir.join(format!("{}.log", sanitize(run_id)));
        let file = std::fs::create_dir_all(&dir)
            .and_then(|_| {
                File::options()
                    .create(true)
                    .append(true)
                    .open(&path)
            })
            .ok();
        Self {
            path: file.as_ref().map(|_| path),
            file: file.map(Mutex::new),
            verbose,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// One line per ledger entry, aligned so `grep` and eyeballs both work.
    pub fn write(&self, entry: &LedgerEntry) {
        let line = format!(
            "{}  it {:>3}  {:<18} {}{}",
            format_utc(entry.created_ms),
            entry.iteration,
            format!("{:?}", entry.kind),
            entry
                .node_id
                .as_ref()
                .map(|n| format!("[{n}] "))
                .unwrap_or_default(),
            entry.detail.replace('\n', " "),
        );
        if self.verbose {
            eprintln!("{line}");
        }
        if let Some(f) = &self.file {
            if let Ok(mut f) = f.lock() {
                let _ = writeln!(f, "{line}");
                let _ = f.flush();
            }
        }
    }
}

/// Run ids come from a timestamp today, but a hand-passed `--run-id` reaches
/// this unchecked, and a path separator in a filename is how a log ends up
/// written somewhere nobody looks.
fn sanitize(run_id: &str) -> String {
    let cleaned: String = run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "run".into()
    } else {
        cleaned
    }
}

/// Store plus run log plus run id: everything needed to write down that
/// something happened.
///
/// Bundling them is what keeps the ledger and the log honest — there is one
/// method that records an event, and it writes to both.
pub struct Recorder<'a, S: Store> {
    pub store: &'a S,
    pub run_id: &'a str,
    pub log: RunLog,
}

impl<'a, S: Store> Recorder<'a, S> {
    pub fn new(store: &'a S, run_id: &'a str, log: RunLog) -> Self {
        Self {
            store,
            run_id,
            log,
        }
    }

    /// Append one event to the ledger and the run log.
    pub fn entry(
        &self,
        iteration: u32,
        kind: LedgerKind,
        detail: impl Into<String>,
        node_id: Option<String>,
    ) {
        let e = LedgerEntry {
            run_id: self.run_id.to_string(),
            iteration,
            kind,
            detail: detail.into(),
            node_id,
            tokens: None,
            cost_usd: None,
            created_ms: loopsmith_memory::now_ms(),
        };
        self.log.write(&e);
        let _ = self.store.append_ledger(&e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_iso_utc() {
        // 2026-08-16T00:00:00Z
        assert_eq!(format_utc(1_786_838_400_000), "2026-08-16T00:00:00Z");
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn a_run_id_cannot_escape_the_logs_directory() {
        assert_eq!(sanitize("../../etc/passwd"), "------etc-passwd");
        assert_eq!(sanitize("run-123"), "run-123");
        assert_eq!(sanitize(""), "run");
    }

    #[test]
    fn entries_land_in_the_logs_directory_not_state() {
        let root = loopsmith_util::testing::temp_dir("runlog");
        let log = RunLog::open(&root, "run-1", false);
        let path = log.path().expect("log opens").to_path_buf();

        log.write(&LedgerEntry {
            run_id: "run-1".into(),
            iteration: 2,
            kind: LedgerKind::NodeSucceeded,
            detail: "served by `echoer`\nwith a newline in it".into(),
            node_id: Some("build".into()),
            tokens: None,
            cost_usd: None,
            created_ms: 1_786_838_400_000,
        });

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(path.starts_with(root.join("logs")), "got {}", path.display());
        assert!(text.contains("2026-08-16T00:00:00Z"), "got: {text}");
        assert!(text.contains("NodeSucceeded"), "got: {text}");
        assert!(text.contains("[build]"), "got: {text}");
        assert_eq!(text.lines().count(), 1, "a newline must not split a record");

        loopsmith_util::testing::cleanup(&root);
    }

    #[test]
    fn an_unwritable_location_degrades_to_a_no_op() {
        // A full disk or a read-only mount must cost the log, not the run.
        let blocked = loopsmith_util::testing::temp_dir("blocked").join("a-file");
        std::fs::write(&blocked, "not a directory").unwrap();

        let log = RunLog::open(&blocked, "run-1", false);
        assert!(log.path().is_none(), "no log file under a regular file");
        log.write(&LedgerEntry {
            run_id: "run-1".into(),
            iteration: 1,
            kind: LedgerKind::RunStarted,
            detail: "this must not panic".into(),
            node_id: None,
            tokens: None,
            cost_usd: None,
            created_ms: 0,
        });
    }
}
