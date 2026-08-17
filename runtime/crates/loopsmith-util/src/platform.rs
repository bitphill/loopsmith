//! What the host actually is, decided at run time rather than at build time.
//!
//! Three facts about a machine change what loopsmith and the scripts it
//! generates are allowed to assume, and none of them is knowable from
//! `cfg!(target_os = …)` alone:
//!
//! - **The bash on `PATH` may be from 2007.** macOS still ships 3.2.57, because
//!   4.0 changed licence. Associative arrays, `${x,,}`, `mapfile`, and `&>>` all
//!   arrived in 4.0, so a detector script written on Linux and run on a Mac
//!   fails with a syntax error rather than a useful message.
//! - **`sed`, `stat`, and `readlink` take different flags** depending on whether
//!   the userland is GNU or BSD. `sed -i` needs an argument on BSD and must not
//!   have one on GNU, which is the single most common way a working script stops
//!   working on the other machine.
//! - **The scheduler is whatever is installed**, not whatever the operating
//!   system is famous for. A container has neither `launchctl` nor `crontab`; a
//!   Mac has both; a systemd host may have `systemctl` and no `crontab`.
//!
//! Everything here probes and reports. Nothing here decides on the caller's
//! behalf, and nothing is cached across processes — a probe costs one `--version`
//! call and a run is not a hot loop.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    FreeBsd,
    Windows,
    Other,
}

impl Os {
    pub fn detect() -> Self {
        // The binary is built for the platform it runs on, so this one really
        // is a compile-time fact. It is wrapped here so callers have a single
        // place to ask, next to the facts that are *not* compile-time.
        match std::env::consts::OS {
            "macos" => Os::MacOs,
            "linux" => Os::Linux,
            "freebsd" | "openbsd" | "netbsd" => Os::FreeBsd,
            "windows" => Os::Windows,
            _ => Os::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Os::MacOs => "macos",
            Os::Linux => "linux",
            Os::FreeBsd => "bsd",
            Os::Windows => "windows",
            Os::Other => "other",
        }
    }

    /// Whether this host runs `.cmd` batch files rather than `#!` scripts.
    pub fn is_windows(self) -> bool {
        self == Os::Windows
    }
}

/// The user's home directory, by whichever variable this operating system uses.
///
/// `HOME` is not set on Windows outside of a POSIX emulation layer, so code that
/// reads it directly silently loses the home directory there — and the failure
/// surfaces as "cannot locate LaunchAgents" or as a loop happily scaffolding
/// itself into a relative path. `USERPROFILE` is tried first on Windows and
/// `HOME` second, because Git Bash and MSYS set `HOME` and it is the more
/// specific answer when both exist.
pub fn home_dir() -> Option<PathBuf> {
    if Os::detect() == Os::Windows {
        if let Some(h) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            return Some(PathBuf::from(h));
        }
        if let Some(p) = std::env::var_os("USERPROFILE").filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(p));
        }
        // Last resort on a bare cmd.exe: HOMEDRIVE + HOMEPATH.
        let drive = std::env::var_os("HOMEDRIVE")?;
        let path = std::env::var_os("HOMEPATH")?;
        let mut s = drive;
        s.push(path);
        return Some(PathBuf::from(s));
    }
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// Which flavour of the core utilities is on `PATH`.
///
/// Not implied by the operating system: Homebrew's `coreutils` puts GNU tools
/// ahead of BSD ones on a Mac, and a stripped container may have busybox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Userland {
    Gnu,
    Bsd,
    Unknown,
}

impl Userland {
    pub fn as_str(self) -> &'static str {
        match self {
            Userland::Gnu => "gnu",
            Userland::Bsd => "bsd",
            Userland::Unknown => "unknown",
        }
    }

    /// How to spell an in-place edit for this userland.
    ///
    /// GNU takes `sed -i`; BSD takes `sed -i ''`. Getting it wrong on BSD
    /// silently consumes the next argument as the backup suffix, which is how a
    /// script ends up editing a file called `-e`.
    pub fn sed_in_place(self) -> &'static [&'static str] {
        match self {
            Userland::Gnu => &["-i"],
            // Unknown is treated as BSD: the explicit empty suffix is accepted
            // by BSD and rejected loudly by GNU, which is the better failure.
            Userland::Bsd | Userland::Unknown => &["-i", ""],
        }
    }

    /// Probe by asking `sed` for a version. GNU answers; BSD refuses.
    pub fn detect() -> Self {
        match Command::new("sed").arg("--version").output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
                if text.contains("gnu") || text.contains("busybox") {
                    Userland::Gnu
                } else {
                    Userland::Unknown
                }
            }
            Ok(_) => Userland::Bsd,
            Err(_) => Userland::Unknown,
        }
    }
}

/// A bash version, as bash itself reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashVersion {
    pub major: u32,
    pub minor: u32,
    /// The whole first line of `bash --version`, for the report.
    pub raw: String,
}

impl BashVersion {
    /// Everything a script may rely on arrived in 4.0: associative arrays,
    /// `${x,,}`, `mapfile`, `&>>`, and `**`. Below that, a script must be
    /// written to POSIX `sh`.
    pub const MODERN_MAJOR: u32 = 4;

    pub fn is_modern(&self) -> bool {
        self.major >= Self::MODERN_MAJOR
    }

    /// Parse the first line of `bash --version`.
    ///
    /// The shape is stable across every release since 2.0:
    /// `GNU bash, version 3.2.57(1)-release (x86_64-apple-darwin24)`.
    pub fn parse(first_line: &str) -> Option<Self> {
        let after = first_line.split("version ").nth(1)?;
        let number: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let mut parts = number.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
        Some(Self {
            major,
            minor,
            raw: first_line.trim().to_string(),
        })
    }

    fn probe(command: &str) -> Option<Self> {
        let out = Command::new(command).arg("--version").output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        Self::parse(text.lines().next()?)
    }
}

/// The whole picture, probed once.
#[derive(Debug, Clone)]
pub struct Platform {
    pub os: Os,
    pub userland: Userland,
    /// `bash` on `PATH`, when there is one. A machine may have none.
    pub bash: Option<BashVersion>,
    /// `/bin/sh`, when it is a bash in POSIX mode. On Debian it is `dash` and
    /// this is `None`, which is worth knowing: dash rejects `[[`, `local -n`,
    /// and `echo -e` that a bash-as-sh would have accepted by accident.
    pub sh_bash: Option<BashVersion>,
    /// Schedulers actually installed, in the order loopsmith would prefer them.
    pub schedulers: Vec<&'static str>,
}

impl Platform {
    pub fn detect() -> Self {
        let os = Os::detect();
        Self {
            os,
            userland: Userland::detect(),
            bash: BashVersion::probe("bash"),
            sh_bash: BashVersion::probe("/bin/sh"),
            schedulers: preferred_schedulers(os)
                .iter()
                .copied()
                .filter(|c| crate::which(c).is_some())
                .collect(),
        }
    }

    /// The scheduler to hand this loop to, or `None` when the machine has none.
    pub fn scheduler(&self) -> Option<&'static str> {
        self.schedulers.first().copied()
    }

    /// Whether a script may use bash 4 syntax on this machine.
    ///
    /// `false` when bash is missing entirely: a script that cannot be run is
    /// not a script that may assume anything.
    pub fn has_modern_bash(&self) -> bool {
        self.bash.as_ref().is_some_and(BashVersion::is_modern)
    }

    /// The reason a generated script sticks to POSIX `sh`, in one line, or
    /// `None` when nothing is holding it back.
    pub fn portability_note(&self) -> Option<String> {
        match &self.bash {
            None => Some(
                "no bash on PATH; generated scripts use POSIX sh and so should yours".into(),
            ),
            Some(b) if !b.is_modern() => Some(format!(
                "bash {}.{} predates 4.0, so associative arrays, ${{x,,}}, and mapfile are \
                 unavailable; generated scripts use POSIX sh and so should yours",
                b.major, b.minor
            )),
            Some(_) => None,
        }
    }
}

/// The schedulers worth looking for on this operating system, best first.
///
/// This is the *candidate* list, not the answer. `Platform::detect` filters it
/// by what is actually on `PATH`, because the operating system a binary was
/// built for says nothing about what the machine running it has installed.
pub fn preferred_schedulers(os: Os) -> &'static [&'static str] {
    match os {
        // launchd is the one that survives a reboot without the user enabling
        // anything else, so it goes first where it exists.
        Os::MacOs => &["launchctl", "crontab"],
        Os::Linux | Os::FreeBsd => &["crontab", "systemctl"],
        // Task Scheduler is the only one of the four that ships with Windows.
        // `crontab` is still worth probing after it: a machine with Cygwin or
        // WSL interop on PATH has a real cron, and preferring the native tool
        // while still finding the other one is the same rule as everywhere else.
        Os::Windows => &["schtasks", "crontab"],
        Os::Other => &["crontab"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bash_version_line_parses_for_every_release_shape() {
        let cases = [
            (
                "GNU bash, version 3.2.57(1)-release (x86_64-apple-darwin24)",
                (3, 2),
            ),
            (
                "GNU bash, version 5.2.21(1)-release (aarch64-unknown-linux-gnu)",
                (5, 2),
            ),
            ("GNU bash, version 4.0.0(1)-release", (4, 0)),
            ("GNU bash, version 5 (x86_64)", (5, 0)),
        ];
        for (line, (major, minor)) in cases {
            let v = BashVersion::parse(line).unwrap_or_else(|| panic!("{line} parses"));
            assert_eq!((v.major, v.minor), (major, minor), "{line}");
            assert_eq!(v.raw, line);
        }
    }

    #[test]
    fn a_line_that_is_not_a_bash_banner_yields_nothing() {
        for line in ["", "zsh 5.9", "GNU bash", "bash: command not found"] {
            assert!(BashVersion::parse(line).is_none(), "{line:?}");
        }
    }

    #[test]
    fn four_is_the_line_between_posix_and_the_rest() {
        assert!(!BashVersion::parse("GNU bash, version 3.2.57(1)-release")
            .unwrap()
            .is_modern());
        assert!(BashVersion::parse("GNU bash, version 4.0.0(1)-release")
            .unwrap()
            .is_modern());
        assert!(BashVersion::parse("GNU bash, version 5.2.21(1)-release")
            .unwrap()
            .is_modern());
    }

    #[test]
    fn the_in_place_flag_differs_by_userland_and_unknown_fails_loudly() {
        assert_eq!(Userland::Gnu.sed_in_place(), &["-i"]);
        assert_eq!(Userland::Bsd.sed_in_place(), &["-i", ""]);
        // Unknown takes the BSD spelling on purpose: GNU rejects the empty
        // suffix with an error, where BSD would silently treat the next
        // argument as a backup suffix.
        assert_eq!(Userland::Unknown.sed_in_place(), &["-i", ""]);
    }

    #[test]
    fn detection_answers_something_for_this_machine() {
        // Whatever this is running on, the probe must not panic and must not
        // claim a scheduler that is not installed.
        let p = Platform::detect();
        assert_ne!(p.os.as_str(), "");
        for s in &p.schedulers {
            assert!(crate::which(s).is_some(), "{s} was reported but is not on PATH");
        }
        if let Some(s) = p.scheduler() {
            assert!(p.schedulers.contains(&s));
        }
    }

    #[test]
    fn every_os_has_a_scheduler_worth_probing_and_windows_gets_the_native_one() {
        for os in [Os::MacOs, Os::Linux, Os::FreeBsd, Os::Windows, Os::Other] {
            assert!(
                !preferred_schedulers(os).is_empty(),
                "{} has no scheduler candidates",
                os.as_str()
            );
        }
        // The bug this pins: Windows fell into the catch-all arm and was offered
        // `crontab`, which it does not have, so `schedule` reported "no
        // scheduler" on a machine with a perfectly good Task Scheduler.
        assert_eq!(preferred_schedulers(Os::Windows).first(), Some(&"schtasks"));
        assert_eq!(preferred_schedulers(Os::MacOs).first(), Some(&"launchctl"));
    }

    #[test]
    fn the_userland_is_probed_and_never_inferred_from_the_operating_system() {
        // Homebrew's coreutils puts GNU tools ahead of BSD ones on a Mac, so
        // `Os::MacOs` does not imply a BSD userland and `Os::Linux` does not
        // imply a GNU one. The only honest answer comes from asking `sed`.
        let probed = Userland::detect();
        let by_probe = match Command::new("sed").arg("--version").output() {
            Ok(out) if out.status.success() => {
                let t = String::from_utf8_lossy(&out.stdout).to_lowercase();
                if t.contains("gnu") || t.contains("busybox") {
                    Userland::Gnu
                } else {
                    Userland::Unknown
                }
            }
            Ok(_) => Userland::Bsd,
            Err(_) => Userland::Unknown,
        };
        assert_eq!(probed, by_probe, "detect() must agree with the raw probe");
    }

    #[test]
    fn the_home_directory_is_found_by_this_platforms_variable() {
        // Whatever this runs on, one of the variables is set, and the answer is
        // absolute. A relative home directory is how a loop ends up scaffolded
        // into whatever the working directory happened to be.
        let home = home_dir().expect("some home variable is set on any real host");
        assert!(
            home.is_absolute(),
            "{} is not absolute",
            home.display()
        );
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn a_missing_bash_is_reported_as_a_portability_constraint() {
        let none = Platform {
            os: Os::Linux,
            userland: Userland::Gnu,
            bash: None,
            sh_bash: None,
            schedulers: vec![],
        };
        assert!(!none.has_modern_bash());
        assert!(none.portability_note().unwrap().contains("no bash on PATH"));

        let old = Platform {
            bash: BashVersion::parse("GNU bash, version 3.2.57(1)-release"),
            ..none.clone()
        };
        assert!(old.portability_note().unwrap().contains("predates 4.0"));

        let new = Platform {
            bash: BashVersion::parse("GNU bash, version 5.2.21(1)-release"),
            ..none
        };
        assert!(new.has_modern_bash());
        assert!(new.portability_note().is_none());
    }
}
