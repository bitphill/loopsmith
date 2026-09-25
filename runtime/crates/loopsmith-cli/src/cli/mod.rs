//! The command surface, and nothing else.
//!
//! Kept apart from `main.rs` so the argument grammar can be read in one sitting
//! without the bodies of twenty-two commands in the way.
//!
//! 1.0 groups them under four nouns — `loop`, `run`, `memory`, `skills` — with
//! `doctor`, `providers`, `web`, and `mcp` left at the top because each is one
//! thing about this machine rather than one thing about a loop. Every 0.3
//! spelling still works; [`alias`] is where that is arranged, and it is the
//! only file that has to go at 2.0.

pub mod alias;

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
    /// Start the browser UI. Identical to the `web` subcommand — both spellings
    /// exist because `--web` is what people reach for and a subcommand is what
    /// the rest of this grammar looks like. Neither is the "real" one.
    #[arg(long, global = false)]
    pub web: bool,

    /// Build a loop by answering questions in the terminal, one at a time.
    /// Identical to the `guided` subcommand. Where `--web` clicks and `new`
    /// hands you a starter file to edit, this walks the whole config with
    /// every field explained in place, and needs no browser — so it works over
    /// SSH and in a bare terminal.
    #[arg(long, global = false)]
    pub guided: bool,

    /// Walk every question with its explanation, whatever was remembered.
    /// Only meaningful with `--guided`; the subcommand takes it too.
    #[arg(long, global = false, conflicts_with = "expert", requires = "guided")]
    pub novice: bool,

    /// Hand me a filled-in config in $EDITOR instead of asking questions.
    /// Only meaningful with `--guided`; the subcommand takes it too.
    #[arg(long, global = false, requires = "guided")]
    pub expert: bool,

    /// Forget which path was remembered and ask again.
    #[arg(long, global = false, conflicts_with_all = ["novice", "expert"], requires = "guided")]
    pub ask: bool,

    /// Absent when `--web` carries the invocation. Every other path requires
    /// one, and [`Cli::resolve`] is where that requirement is enforced, so the
    /// error message can name the flag instead of clap's generic complaint.
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Collapse the two spellings into one command.
    ///
    /// `--web` and `web` mean exactly the same thing. Resolving here rather
    /// than in `dispatch` keeps `dispatch` a pure match over `Command` and
    /// leaves one place that knows the two spellings are the same.
    pub fn resolve(self) -> Result<Command, String> {
        // `--guided` is a synonym for the subcommand, so it takes the
        // subcommand's own flags too. Anything else would make the short
        // spelling a lesser one.
        let (novice, expert, ask) = (self.novice, self.expert, self.ask);
        match (self.web, self.guided, self.command) {
            (true, false, None) => Ok(Command::Web { port: None, no_open: false }),
            (false, true, None) => Ok(Command::Loop {
                action: LoopAction::Guided {
                    path: None,
                    edit: None,
                    novice,
                    expert,
                    ask,
                },
            }),
            // Two different UIs onto the same config. Picking one for the user
            // would guess wrong half the time.
            (true, true, _) => Err(
                "`--web` and `--guided` are two front ends for the same thing — \
                 a browser and a terminal wizard. Pick one."
                    .into(),
            ),
            // `loopsmith --web run loop.yaml` is a contradiction, not a
            // shorthand. Refusing beats silently picking one.
            (true, false, Some(_)) => Err(
                "`--web` starts the browser UI and takes no subcommand. \
                 Use `loopsmith web`, or drop `--web`."
                    .into(),
            ),
            (false, true, Some(_)) => Err(
                "`--guided` starts the terminal wizard and takes no subcommand. \
                 Use `loopsmith loop guided`, or drop `--guided`."
                    .into(),
            ),
            (false, false, Some(c)) => Ok(c),
            (false, false, None) => Err(
                "no command given. `loopsmith --help` lists them; `loopsmith loop guided` \
                 builds a loop by asking questions in the terminal, and `loopsmith web` \
                 does the same in a browser."
                    .into(),
            ),
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Make and maintain loop configs: create, edit, check, convert.
    Loop {
        #[command(subcommand)]
        action: LoopAction,
    },
    /// Start a loop and everything that follows from having started one.
    ///
    /// `loopsmith run <config>` still runs the loop — that spelling is in
    /// every generated launcher and every crontab line — and is the same
    /// thing as `loopsmith run start <config>`.
    Run {
        #[command(subcommand)]
        action: RunAction,
    },
    /// What this loop remembers across runs, and the human half of promotion.
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Discover, install, and score sub-agents.
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Report what this machine is, and what that stops you doing.
    Doctor {
        /// Also check what this config needs that the machine may not have.
        config: Option<PathBuf>,
    },
    /// Report which providers are usable right now.
    Providers { config: PathBuf },
    /// Build, run, and watch loops from a browser. Same thing as `--web`.
    ///
    /// Everything this binary does from a terminal, done by clicking: pick a
    /// provider it found on this machine, fill in every field with its
    /// explanation in place, and press a button. Binds to localhost only.
    Web {
        /// Port to serve on. Defaults to 3000, and steps up one at a time
        /// until a free port is found rather than failing on a busy one.
        #[arg(long)]
        port: Option<u16>,
        /// Print the URL instead of opening a browser tab.
        #[arg(long)]
        no_open: bool,
    },
    /// Serve the local MCP server on stdio.
    Mcp {
        #[arg(long, default_value = "state")]
        state: PathBuf,
    },
}

/// `loopsmith loop …` — everything that concerns the config file itself.
#[derive(Subcommand)]
pub enum LoopAction {
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
        /// Initialise a git repository in the new directory, with one commit.
        ///
        /// This is what lets isolated nodes have a worktree each. Without a
        /// repository they all share one directory, which is fine for a single
        /// builder and destructive for two running at once.
        #[arg(long)]
        git: bool,
    },
    /// Build a loop in the terminal: guided questions, or your editor. Same as `--guided`.
    ///
    /// Every section, asked one field at a time, each with the explanation the
    /// field carries in the web UI — the two front ends ask the same list.
    /// Providers this machine already has are offered as a numbered menu;
    /// everything else is a prompt with its default in `[brackets]` — press
    /// Enter to take it. `:back`, `:next`, `:help`, and `:quit` work at any
    /// prompt. The validator runs before anything is written, and what it
    /// found is shown; writing anyway is a choice you make, not the default.
    Guided {
        /// Directory for the new loop. Omit and the wizard asks for it.
        #[arg(value_name = "DIR")]
        path: Option<PathBuf>,
        /// Load an existing config and change it, instead of starting from
        /// the defaults. The result is written back over the same file.
        #[arg(long, value_name = "FILE")]
        edit: Option<PathBuf>,
        /// Walk every question with its explanation, whatever was remembered.
        #[arg(long, conflicts_with = "expert")]
        novice: bool,
        /// Hand me a filled-in config in $EDITOR instead of asking questions.
        #[arg(long)]
        expert: bool,
        /// Forget which path was remembered and ask again.
        #[arg(long, conflicts_with_all = ["novice", "expert"])]
        ask: bool,
    },
    /// Check a config against the model.
    Validate {
        config: PathBuf,
        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Show waves, critical path, and predicted speedup without running.
    Plan { config: PathBuf },
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
    /// Rewrite a 0.3 config into the 1.0 shape.
    ///
    /// Optional: a 0.3 config still loads. This makes the file say what the
    /// loader already understands, which stops the deprecation notices.
    Migrate {
        config: PathBuf,
        /// Report what would change and exit non-zero. Writes nothing.
        #[arg(long)]
        check: bool,
        /// Replace the file. Without this the result goes to stdout.
        #[arg(long)]
        write: bool,
    },
    /// Print the consolidated permission grant this config needs.
    Permissions {
        config: PathBuf,
        /// Merge into .claude/settings.local.json instead of printing.
        #[arg(long)]
        write: Option<PathBuf>,
    },
}

/// `loopsmith run …` — one run of a loop, from starting it to reading what it
/// left behind.
#[derive(Subcommand)]
pub enum RunAction {
    /// Run the loop. `loopsmith run <config>` is the same command.
    Start {
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
        /// The run's open escalations have been dealt with: clear them, and
        /// give each escalated node its revisions back. Without this a resume
        /// leaves them open and the nodes held.
        #[arg(long)]
        answer: bool,
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
    /// Show what the loop wants changed about itself. It cannot apply these.
    Proposals { config: PathBuf, run_id: String },
    /// Remove the git worktrees this loop created.
    Prune { config: PathBuf },
}

#[derive(Subcommand)]
pub enum MemoryAction {
    /// Every record, promoted or not, with where it came from.
    List {
        config: PathBuf,
        /// semantic, procedural, or failure. Omit for all three.
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Promote a record so later runs reuse it. The only way a namespace whose
    /// rule is `human_approval` ever promotes anything.
    Promote {
        config: PathBuf,
        namespace: String,
        key: String,
    },
    /// Delete a record.
    Forget {
        config: PathBuf,
        namespace: String,
        key: String,
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
