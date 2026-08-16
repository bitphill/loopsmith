//! Sub-agent acquisition, and the experiment loop that makes it useful.
//!
//! A loop cannot reason its way to knowing which sub-agents earn their keep.
//! It has to try one, watch the gate, and keep what correlates with satisfied
//! goals. That is the whole self-evolution story here, and it is deliberately
//! narrow: acquisition is an action the loop may take, but *adopting* a skill
//! into the config is a proposal a human applies.
//!
//! Three sources, tried in the configured order:
//!
//! 1. **Installed** — already on disk under a skills directory.
//! 2. **Marketplace** — the `claudemarketplaces.com` index (plugin bundles)
//!    and the `skills` CLI (individual skills).
//! 3. **Generate** — write a minimal skill from the requirement.
//!
//! Network access is shelled out to `curl` and `npx` rather than linked in.
//! That keeps the crate dependency-free, and means a machine without either
//! degrades to "installed only" instead of failing to build.

use loopsmith_core::{AcquisitionSource, SkillPolicy};
use loopsmith_memory::{score_skills, SkillScore, SkillTrial};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod marketplace;
pub use marketplace::{search_marketplace, MarketplaceEntry};

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("`{cmd}` failed ({code}): {stderr}")]
    Command {
        cmd: String,
        code: i32,
        stderr: String,
    },
    #[error("`{0}` is not available on PATH")]
    Missing(String),
    #[error("refused: {0}")]
    Refused(String),
}

pub type Result<T> = std::result::Result<T, SkillError>;

/// Where a skill came from. Recorded on every trial so a skill's track record
/// carries its provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Installed,
    Marketplace,
    Generated,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Installed => "installed",
            Source::Marketplace => "marketplace",
            Source::Generated => "generated",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSkill {
    pub name: String,
    pub source: Source,
    pub path: PathBuf,
    /// True when the skill sits in quarantine and has not been promoted.
    pub quarantined: bool,
}

/// Directories searched for an already-installed skill, nearest first.
pub fn skill_search_paths(project_root: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        project_root.join(".claude/skills"),
        project_root.join("generated-skills"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        v.push(PathBuf::from(home).join(".claude/skills"));
    }
    v
}

/// Is this skill already on disk? A skill is a directory containing SKILL.md.
pub fn find_installed(name: &str, project_root: &Path) -> Option<ResolvedSkill> {
    for dir in skill_search_paths(project_root) {
        let candidate = dir.join(name);
        if candidate.join("SKILL.md").is_file() {
            // On disk we can see *where* it is, not where it came from, so
            // report Installed and let `quarantined` carry the caveat.
            let quarantined = dir.ends_with("generated-skills");
            return Some(ResolvedSkill {
                name: name.to_string(),
                source: Source::Installed,
                path: candidate,
                quarantined,
            });
        }
    }
    None
}

/// Every skill currently visible, deduplicated by name with the nearest
/// directory winning.
pub fn list_installed(project_root: &Path) -> Vec<ResolvedSkill> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for dir in skill_search_paths(project_root) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if !p.join("SKILL.md").is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !seen.insert(name.to_string()) {
                continue;
            }
            let quarantined = dir.ends_with("generated-skills");
            out.push(ResolvedSkill {
                name: name.to_string(),
                source: Source::Installed,
                path: p,
                quarantined,
            });
        }
    }
    out
}

fn run(cmd: &str, args: &[&str], cwd: &Path) -> Result<String> {
    if which(cmd).is_none() {
        return Err(SkillError::Missing(cmd.to_string()));
    }
    let out = Command::new(cmd).args(args).current_dir(cwd).output()?;
    if !out.status.success() {
        return Err(SkillError::Command {
            cmd: format!("{cmd} {}", args.join(" ")),
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|d| {
        let c = d.join(cmd);
        c.is_file().then_some(c)
    })
}

/// A name that is safe to use as a directory and to pass to a package manager.
/// Anything else is refused rather than sanitised, because a silently rewritten
/// skill name installs something the caller did not ask for.
pub fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '@' || c == '.')
        && !name.contains("..")
        && !name.starts_with('/')
        && !name.starts_with('-')
}

/// Names that must never be auto-installed regardless of trust score.
pub fn is_blocklisted(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["credential", "secret", "exfil", "keylog", "password", "token-steal"]
        .iter()
        .any(|bad| n.contains(bad))
}

/// Install a skill from the `skills` CLI into the quarantine directory.
///
/// Quarantine is the point: an acquired sub-agent runs with whatever the
/// permission grant allowed, so it lands somewhere inert until a human moves
/// it. `--dir` keeps it out of the global skills path.
pub fn install_from_cli(spec: &str, quarantine: &Path) -> Result<ResolvedSkill> {
    if !is_safe_name(spec) {
        return Err(SkillError::Refused(format!(
            "`{spec}` is not a safe skill spec"
        )));
    }
    if is_blocklisted(spec) {
        return Err(SkillError::Refused(format!(
            "`{spec}` matches the never-auto-install list"
        )));
    }
    std::fs::create_dir_all(quarantine)?;
    run(
        "npx",
        &["--yes", "skills", "add", spec, "--dir", ".", "-y"],
        quarantine,
    )?;
    let name = spec
        .rsplit('@')
        .next()
        .unwrap_or(spec)
        .rsplit('/')
        .next()
        .unwrap_or(spec)
        .to_string();
    let path = quarantine.join(&name);
    Ok(ResolvedSkill {
        name,
        source: Source::Marketplace,
        path,
        quarantined: true,
    })
}

/// Write a minimal skill from a requirement. The last resort, used when
/// nothing installed or published fits.
pub fn generate(name: &str, purpose: &str, quarantine: &Path) -> Result<ResolvedSkill> {
    if !is_safe_name(name) {
        return Err(SkillError::Refused(format!("`{name}` is not a safe name")));
    }
    let dir = quarantine.join(name);
    std::fs::create_dir_all(&dir)?;
    let body = format!(
        "---\nname: {name}\ndescription: >\n  {purpose} Generated by loopsmith because no installed or\n  published skill matched this requirement. Review before promoting.\n---\n\n# {name}\n\n{purpose}\n\n## Approach\n\nState the steps you take, then the check that proves the work is done.\nA step whose result cannot be checked does not belong here.\n\n## Output\n\nReport what you produced and the evidence for it. A claim without evidence\ncannot be acted on by the gate.\n"
    );
    std::fs::write(dir.join("SKILL.md"), body)?;
    Ok(ResolvedSkill {
        name: name.to_string(),
        source: Source::Generated,
        path: dir,
        quarantined: true,
    })
}

/// Resolve one required skill, walking the configured acquisition order.
pub fn acquire(
    name: &str,
    purpose: &str,
    policy: &SkillPolicy,
    project_root: &Path,
) -> Result<ResolvedSkill> {
    let quarantine = project_root.join(&policy.quarantine_dir);

    for step in &policy.acquisition_order {
        match step {
            AcquisitionSource::Installed => {
                if let Some(found) = find_installed(name, project_root) {
                    return Ok(found);
                }
            }
            AcquisitionSource::Marketplace => {
                // Try the skills CLI by bare name first; the marketplace index
                // is for discovery, and the caller passes a resolved spec when
                // it has one.
                match install_from_cli(name, &quarantine) {
                    Ok(r) => return Ok(r),
                    Err(SkillError::Refused(m)) => return Err(SkillError::Refused(m)),
                    Err(_) => continue,
                }
            }
            AcquisitionSource::Generate => {
                return generate(name, purpose, &quarantine);
            }
        }
    }
    Err(SkillError::Missing(name.to_string()))
}

/// Which skills should this node use next?
///
/// Configured skills always run. On top of that, a skill with a proven record
/// is suggested, and a skill with a proven *bad* record is suggested for
/// removal. `min_trials` exists because one lucky run is not evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub adopt: Vec<String>,
    pub drop: Vec<String>,
}

pub fn recommend(
    configured: &[String],
    trials: &[SkillTrial],
    min_trials: usize,
    adopt_above: f64,
    drop_below: f64,
) -> Recommendation {
    let scored: Vec<SkillScore> = score_skills(trials);
    let cfg: BTreeSet<&str> = configured.iter().map(|s| s.as_str()).collect();

    let mut adopt = Vec::new();
    let mut drop = Vec::new();
    for s in &scored {
        if s.trials < min_trials {
            continue;
        }
        let rate = s.satisfaction_rate();
        if rate >= adopt_above && !cfg.contains(s.skill.as_str()) {
            adopt.push(s.skill.clone());
        } else if rate <= drop_below && cfg.contains(s.skill.as_str()) {
            drop.push(s.skill.clone());
        }
    }
    Recommendation { adopt, drop }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_memory::now_ms;

    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn tmp(tag: &str) -> PathBuf {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "loopsmith-skills-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_skill(root: &Path, dir: &str, name: &str) {
        let d = root.join(dir).join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), "---\nname: x\n---\nbody").unwrap();
    }

    #[test]
    fn an_installed_skill_is_found_in_the_project() {
        let root = tmp("installed");
        make_skill(&root, ".claude/skills", "tdd");
        let r = find_installed("tdd", &root).expect("found");
        assert_eq!(r.source, Source::Installed);
        assert!(!r.quarantined);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_quarantined_skill_is_marked_as_such() {
        let root = tmp("quarantined");
        make_skill(&root, "generated-skills", "scraper");
        let r = find_installed("scraper", &root).expect("found");
        assert!(r.quarantined, "quarantine must be visible to the caller");
        // Provenance is not knowable from the filesystem; only location is.
        assert_eq!(r.source, Source::Installed);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_directory_without_skill_md_is_not_a_skill() {
        let root = tmp("nomd");
        std::fs::create_dir_all(root.join(".claude/skills/empty")).unwrap();
        assert!(find_installed("empty", &root).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn listing_prefers_the_nearest_directory_on_a_name_clash() {
        let root = tmp("clash");
        make_skill(&root, ".claude/skills", "dup");
        make_skill(&root, "generated-skills", "dup");
        let all = list_installed(&root);
        let dups: Vec<&ResolvedSkill> = all.iter().filter(|s| s.name == "dup").collect();
        assert_eq!(dups.len(), 1, "name must not appear twice");
        assert!(!dups[0].quarantined, "project skill should win over quarantine");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unsafe_names_are_refused_rather_than_sanitised() {
        for bad in ["../escape", "", "-rf", "a/../../b", &"x".repeat(80)] {
            assert!(!is_safe_name(bad), "{bad:?} should be unsafe");
        }
        for ok in ["tdd", "vercel-labs/agent-skills@react", "my_skill.v2"] {
            assert!(is_safe_name(ok), "{ok:?} should be safe");
        }
    }

    #[test]
    fn credential_shaped_skills_are_never_auto_installed() {
        assert!(is_blocklisted("aws-credential-helper"));
        assert!(is_blocklisted("KeyLogger"));
        assert!(!is_blocklisted("tdd"));
        let root = tmp("blocked");
        let e = install_from_cli("secret-stealer", &root.join("q")).unwrap_err();
        assert!(matches!(e, SkillError::Refused(_)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn generate_writes_a_usable_skill_into_quarantine() {
        let root = tmp("generate");
        let q = root.join("generated-skills");
        let r = generate("csv-tidier", "Clean malformed CSV exports.", &q).unwrap();
        assert!(r.quarantined);
        assert_eq!(r.source, Source::Generated);
        let body = std::fs::read_to_string(r.path.join("SKILL.md")).unwrap();
        assert!(body.starts_with("---\nname: csv-tidier"));
        assert!(body.contains("Review before promoting"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn acquire_prefers_an_installed_skill_over_installing_one() {
        let root = tmp("prefer");
        make_skill(&root, ".claude/skills", "already-here");
        let policy = SkillPolicy::default();
        let r = acquire("already-here", "p", &policy, &root).unwrap();
        assert_eq!(r.source, Source::Installed);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn acquire_falls_through_to_generation_when_nothing_is_found() {
        let root = tmp("fallthrough");
        let policy = SkillPolicy {
            // Skip the marketplace so the test does not touch the network.
            acquisition_order: vec![AcquisitionSource::Installed, AcquisitionSource::Generate],
            ..SkillPolicy::default()
        };
        let r = acquire("brand-new", "Do a new thing.", &policy, &root).unwrap();
        assert_eq!(r.source, Source::Generated);
        assert!(r.path.join("SKILL.md").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    fn trial(skill: &str, ok: bool, rate: f64) -> SkillTrial {
        SkillTrial {
            run_id: "r".into(),
            iteration: 1,
            node_id: "n".into(),
            skill: skill.into(),
            source: "marketplace".into(),
            pass_rate: rate,
            satisfied: ok,
            tokens: None,
            created_ms: now_ms(),
        }
    }

    #[test]
    fn a_skill_that_works_is_recommended_for_adoption() {
        let trials = vec![
            trial("helper", true, 1.0),
            trial("helper", true, 1.0),
            trial("helper", true, 0.9),
        ];
        let r = recommend(&[], &trials, 3, 0.8, 0.2);
        assert_eq!(r.adopt, vec!["helper".to_string()]);
        assert!(r.drop.is_empty());
    }

    #[test]
    fn a_configured_skill_that_never_helps_is_recommended_for_removal() {
        let trials = vec![
            trial("deadweight", false, 0.1),
            trial("deadweight", false, 0.0),
            trial("deadweight", false, 0.2),
        ];
        let r = recommend(&["deadweight".into()], &trials, 3, 0.8, 0.2);
        assert_eq!(r.drop, vec!["deadweight".to_string()]);
        assert!(r.adopt.is_empty());
    }

    #[test]
    fn one_lucky_run_is_not_evidence() {
        let trials = vec![trial("fluke", true, 1.0)];
        let r = recommend(&[], &trials, 3, 0.8, 0.2);
        assert!(
            r.adopt.is_empty(),
            "a single trial must not drive a config change"
        );
    }

    #[test]
    fn an_already_configured_skill_is_not_re_adopted() {
        let trials = vec![
            trial("kept", true, 1.0),
            trial("kept", true, 1.0),
            trial("kept", true, 1.0),
        ];
        let r = recommend(&["kept".into()], &trials, 3, 0.8, 0.2);
        assert!(r.adopt.is_empty());
        assert!(r.drop.is_empty());
    }
}
