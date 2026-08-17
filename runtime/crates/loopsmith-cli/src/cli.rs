//! The command surface, and nothing else.
//!
//! Kept apart from `main.rs` so the argument grammar can be read in one sitting
//! without the bodies of sixteen commands in the way.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "loopsmith",
    version,
    about = "Plan, run, and gate self-evolving agent loops",
    long_about = "loopsmith owns the parts of an agent loop that must not be a matter of \
opinion: the dependency graph, the persistent ledger, and the gate that decides whether a \
goal is actually satisfied. Models supply judgment; this binary supplies the truth."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new purpose-specific loop at a path.
    New {
        /// Directory for the new loop. Required: a loop owns durable state and
        /// needs a home of its own.
        #[arg(short = 'p', long = "path", value_name = "DIR")]
        path: PathBuf,
        /// Loop name. Defaults to the directory name.
        #[arg(short, long)]
        name: Option<String>,
        /// One line on what this loop is for.
        #[arg(long, default_value = "a loopsmith loop")]
        purpose: String,
        /// Write into a non-empty directory.
        #[arg(long)]
        force: bool,
        /// Use this complete config instead of the starter. Grammar is chosen
        /// by the file's extension.
        #[arg(long, value_name = "FILE", conflicts_with = "config_stdin")]
        config_file: Option<PathBuf>,
        /// Read the complete config from stdin instead of the starter.
        #[arg(long)]
        config_stdin: bool,
        /// Treat a config read from stdin as Markdown rather than YAML.
        #[arg(long)]
        markdown: bool,
    },
    /// Check a config against the A–H model.
    Validate {
        config: PathBuf,
        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Translate a config between YAML and Markdown. Both are the same model.
    Convert {
        config: PathBuf,
        /// Write here instead of to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Emit YAML even when the input is already YAML.
        #[arg(long)]
        to_yaml: bool,
    },
    /// Show waves, critical path, and predicted speedup without running.
    Plan { config: PathBuf },
    /// Run the loop.
    Run {
        config: PathBuf,
        #[arg(long)]
        run_id: Option<String>,
        /// Plan and log without invoking any provider.
        #[arg(long)]
        dry_run: bool,
        /// Do not acquire missing sub-agents; nodes run without them.
        #[arg(long)]
        no_acquire: bool,
        /// Mirror the run log to stderr as it is written.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Continue a run from its last checkpoint.
    Resume {
        config: PathBuf,
        run_id: String,
        /// Mirror the run log to stderr as it is written.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Current gate rulings for a run.
    Status { config: PathBuf, run_id: String },
    /// Print the append-only ledger for a run.
    Ledger {
        config: PathBuf,
        run_id: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Evaluate the gate once against the current working tree.
    Gate {
        config: PathBuf,
        /// Goal name, or `overall`.
        #[arg(long, default_value = "overall")]
        target: String,
        #[arg(long, default_value = ".")]
        workdir: PathBuf,
    },
    /// Report which providers are usable right now.
    Providers { config: PathBuf },
    /// Report what this machine is, and what that stops you doing.
    Doctor {
        /// Also check what this config needs that the machine may not have.
        config: Option<PathBuf>,
    },
    /// Print the consolidated permission grant this config needs.
    Permissions {
        config: PathBuf,
        /// Merge into .claude/settings.local.json instead of printing.
        #[arg(long)]
        write: Option<PathBuf>,
    },
    /// Stay resident and run the loop whenever a trigger fires. This is what
    /// makes a loop live for weeks rather than for one invocation.
    Watch {
        config: PathBuf,
        /// Stop after this many runs. Omit to run until interrupted.
        #[arg(long)]
        max_runs: Option<u32>,
        /// Report what would fire, then exit without running anything.
        #[arg(long)]
        check: bool,
    },
    /// Hand the schedule to the operating system so it survives a reboot.
    Schedule {
        config: PathBuf,
        /// Write the launchd agent or crontab line instead of printing it.
        #[arg(long)]
        install: bool,
    },
    /// Discover, install, and score sub-agents.
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Show what the loop wants changed about itself. It cannot apply these.
    Proposals { config: PathBuf, run_id: String },
    /// Remove the git worktrees this loop created.
    Prune { config: PathBuf },
    /// Serve the local MCP server on stdio.
    Mcp {
        #[arg(long, default_value = "state")]
        state: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum SkillsAction {
    /// Sub-agents already visible to this loop.
    List {
        config: PathBuf,
        /// Include the ~/.claude/skills directory, which is usually large.
        #[arg(long)]
        all: bool,
    },
    /// Search claudemarketplaces.com and the skills CLI.
    Search {
        /// Words to match against repo, description, categories, keywords.
        terms: Vec<String>,
        #[arg(long, default_value_t = 100)]
        min_stars: u64,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Install a skill into the loop's quarantine directory.
    Acquire {
        config: PathBuf,
        /// Skill name, or an `owner/repo@skill` spec.
        name: String,
    },
    /// Install everything this loop declares under `default_skills` (section J).
    Install { config: PathBuf },
    /// Rank sub-agents by the gate outcomes that followed their use.
    Scores { config: PathBuf },
}
