//! The terminal itself: reading a line, colouring a word, and the four
//! navigation commands that work at every prompt.
//!
//! Everything the wizard shows the user goes through here, and every answer it
//! reads comes back through here. Two things are centralised on purpose:
//!
//!  - **The reserved commands.** `:back`, `:next`, `:help`, and `:quit` (and
//!    their one-letter forms) are recognised in exactly one place, so no field
//!    can forget to honour them and none can accidentally treat `:quit` typed
//!    into a text box as the answer. They are colon-prefixed so a real answer of
//!    "back" or "next" is still typeable — only `:back` navigates.
//!  - **Interactive vs piped.** A TTY gets colour, prompts, and re-asks on bad
//!    input. A pipe (a test, a script) gets none of that: each line of stdin is
//!    consumed as the next answer. Both read the same stdin the same way, so the
//!    wizard's logic never branches on which it is talking to.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A control signal a prompt returns instead of a value.
///
/// These bubble up through the section drivers: `Back` is caught by the nearest
/// driver that has an earlier field to return to, `Quit` is caught once at the
/// top by [`super::run`], which offers to save a draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    /// Return to the previous field.
    Back,
    /// Leave the wizard.
    Quit,
}

/// What a single read resolved to, before a field interprets it.
enum Token {
    /// A line of input, already trimmed of its trailing newline. May be empty.
    Text(String),
    /// `:help` — show the field's explanation again and re-ask.
    Help,
    /// A navigation command.
    Nav(Nav),
}

/// One option in a numbered menu.
pub struct Choice {
    /// What is written into the config if this is picked.
    pub value: String,
    /// What the user sees on the numbered line.
    pub label: String,
    /// An optional dimmed note after the label.
    pub note: Option<String>,
}

impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Choice { value: value.into(), label: label.into(), note: None }
    }
    pub fn noted(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// The terminal the wizard talks through.
pub struct Io {
    color: bool,
    interactive: bool,
    stdin: std::io::Stdin,
    /// Set from a background Ctrl-C handler. Checked after every read so an
    /// interrupt turns into the same quit menu `:quit` opens.
    interrupted: Arc<AtomicBool>,
}

impl Io {
    /// Build the terminal, wiring up the Ctrl-C handler.
    ///
    /// The returned `Arc<AtomicBool>` is shared with the signal handler. It is
    /// held by `Io` so nothing else has to; callers never touch it.
    pub fn new() -> Self {
        let stdin = std::io::stdin();
        let interactive = stdin.is_terminal() && std::io::stdout().is_terminal();
        let interrupted = Arc::new(AtomicBool::new(false));
        // A handler that only flips a flag. The default action (terminate) is
        // suppressed by installing any handler at all, so the wizard survives
        // the interrupt and can offer to save. On Unix the flag is noticed
        // because a blocked read returns `Interrupted`; either way, `:quit`
        // remains the always-reliable path if a platform will not unblock the
        // read until the next Enter.
        let flag = interrupted.clone();
        // A second handler install would error (only one is allowed per
        // process); ignore that so a repeated `guided` in one test process does
        // not panic.
        let _ = ctrlc::set_handler(move || {
            flag.store(true, Ordering::SeqCst);
        });
        Io {
            color: color_enabled(interactive),
            interactive,
            stdin,
            interrupted,
        }
    }

    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    // -- reading -----------------------------------------------------------

    /// Read one line and classify it. `Err(Nav::Quit)` on end-of-input.
    fn read_token(&mut self) -> Result<Token, Nav> {
        if self.interrupted.swap(false, Ordering::SeqCst) {
            return Ok(Token::Nav(Nav::Quit));
        }
        let mut line = String::new();
        let n = loop {
            match self.stdin.lock().read_line(&mut line) {
                Ok(n) => break n,
                // A signal interrupted the blocking read. If it was our Ctrl-C,
                // the flag is set; otherwise retry the read.
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    if self.interrupted.swap(false, Ordering::SeqCst) {
                        return Ok(Token::Nav(Nav::Quit));
                    }
                    line.clear();
                    continue;
                }
                // A terminal that has gone away is not recoverable; treat it as
                // a request to leave rather than looping on the error.
                Err(_) => return Err(Nav::Quit),
            }
        };
        if n == 0 {
            // EOF: Ctrl-D at a TTY, or the end of a piped script.
            return Err(Nav::Quit);
        }
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        Ok(match reserved(&trimmed) {
            Some(Reserved::Back) => Token::Nav(Nav::Back),
            Some(Reserved::Quit) => Token::Nav(Nav::Quit),
            Some(Reserved::Help) => Token::Help,
            Some(Reserved::Next) => Token::Text(String::new()),
            None => Token::Text(trimmed),
        })
    }

    // -- asking ------------------------------------------------------------

    /// A free-text field. `default` is used on empty input; `validate` gates the
    /// value and its message is shown before re-asking.
    pub fn ask_text(
        &mut self,
        label: &str,
        help: &[&str],
        default: Option<&str>,
        validate: &dyn Fn(&str) -> Result<(), String>,
    ) -> Result<String, Nav> {
        loop {
            self.render_help(label, help, default);
            match self.read_token()? {
                Token::Nav(n) => return Err(n),
                Token::Help => {
                    self.render_full_help(label, help);
                    continue;
                }
                Token::Text(s) => {
                    let value = if s.is_empty() {
                        default.unwrap_or("").to_string()
                    } else {
                        s
                    };
                    match validate(&value) {
                        Ok(()) => return Ok(value),
                        Err(why) => {
                            self.error(&why);
                            continue;
                        }
                    }
                }
            }
        }
    }

    /// A yes/no field. Empty input takes `default`.
    pub fn ask_bool(&mut self, label: &str, help: &[&str], default: bool) -> Result<bool, Nav> {
        let hint = if default { "[Y/n]" } else { "[y/N]" };
        loop {
            self.write_prompt(&format!("{label} {}", self.dim(hint)), help);
            match self.read_token()? {
                Token::Nav(n) => return Err(n),
                Token::Help => {
                    self.render_full_help(label, help);
                    continue;
                }
                Token::Text(s) => match parse_bool(&s, default) {
                    Some(b) => return Ok(b),
                    None => {
                        self.error("answer y or n (or press Enter for the default)");
                        continue;
                    }
                },
            }
        }
    }

    /// A numbered menu. Returns the chosen [`Choice::value`]. The user types the
    /// number; the exact label is also accepted, so a copied option still works.
    /// `default` is a 0-based index used on empty input.
    pub fn ask_select(
        &mut self,
        label: &str,
        help: &[&str],
        choices: &[Choice],
        default: Option<usize>,
    ) -> Result<String, Nav> {
        assert!(!choices.is_empty(), "a menu needs at least one option");
        loop {
            if !help.is_empty() {
                for line in help {
                    self.println(&self.dim(line));
                }
            }
            for (i, c) in choices.iter().enumerate() {
                let num = self.bold(&format!("{:>2}", i + 1));
                let note = c
                    .note
                    .as_deref()
                    .map(|n| format!("  {}", self.dim(n)))
                    .unwrap_or_default();
                self.println(&format!("  {num}. {}{note}", c.label));
            }
            let hint = match default {
                Some(i) => format!("[{}]", i + 1),
                None => "1-{}".replace("{}", &choices.len().to_string()),
            };
            self.write_prompt(&format!("{label} {}", self.dim(&hint)), &[]);
            match self.read_token()? {
                Token::Nav(n) => return Err(n),
                Token::Help => {
                    self.render_full_help(label, help);
                    continue;
                }
                Token::Text(s) => {
                    let s = s.trim();
                    if s.is_empty() {
                        if let Some(i) = default {
                            return Ok(choices[i].value.clone());
                        }
                        self.error("type the number of an option");
                        continue;
                    }
                    if let Ok(n) = s.parse::<usize>() {
                        if (1..=choices.len()).contains(&n) {
                            return Ok(choices[n - 1].value.clone());
                        }
                        self.error(&format!("pick a number from 1 to {}", choices.len()));
                        continue;
                    }
                    // Label fallback (Q9): an exact, case-insensitive match on
                    // the visible label or the stored value.
                    if let Some(c) = choices.iter().find(|c| {
                        c.label.eq_ignore_ascii_case(s) || c.value.eq_ignore_ascii_case(s)
                    }) {
                        return Ok(c.value.clone());
                    }
                    self.error(&format!("type a number from 1 to {}", choices.len()));
                }
            }
        }
    }

    // -- rendering ---------------------------------------------------------

    fn render_help(&self, label: &str, help: &[&str], default: Option<&str>) {
        if !help.is_empty() {
            for line in help {
                self.println(&self.dim(line));
            }
        }
        let d = match default {
            Some(d) if !d.is_empty() => format!(" {}", self.dim(&format!("[{d}]"))),
            Some(_) => format!(" {}", self.dim("[optional, Enter to skip]")),
            None => String::new(),
        };
        self.write_prompt(&format!("{label}{d}"), &[]);
    }

    fn render_full_help(&self, label: &str, help: &[&str]) {
        self.println("");
        self.println(&self.bold(label));
        for line in help {
            self.println(&format!("  {line}"));
        }
        self.println(&self.dim(
            "  commands: :back  :next (keep default)  :quit  :help",
        ));
        self.println("");
    }

    /// Write the prompt line without a trailing newline, so the cursor sits
    /// after it. In non-interactive mode nothing is written — a pipe has no eyes.
    fn write_prompt(&self, text: &str, help: &[&str]) {
        if !self.interactive {
            return;
        }
        if !help.is_empty() {
            for line in help {
                self.println(&self.dim(line));
            }
        }
        print!("{} {} ", self.cyan("›"), text);
        let _ = std::io::stdout().flush();
    }

    pub fn println(&self, text: &str) {
        if self.interactive {
            println!("{text}");
        }
    }

    /// A heading between sections. Always visible, blank-line padded.
    pub fn heading(&self, text: &str) {
        if !self.interactive {
            return;
        }
        println!("\n{}", self.bold(&self.cyan(text)));
        println!("{}", self.dim(&"─".repeat(text.chars().count().min(60))));
    }

    pub fn note(&self, text: &str) {
        self.println(&self.dim(&format!("  {text}")));
    }

    pub fn error(&self, text: &str) {
        if self.interactive {
            println!("  {} {}", self.red("✗"), self.red(text));
        }
    }

    pub fn success(&self, text: &str) {
        self.println(&format!("  {} {}", self.green("✓"), text));
    }

    // -- colour ------------------------------------------------------------

    fn wrap(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
    pub fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub fn cyan(&self, s: &str) -> String {
        self.wrap("36", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }
}

enum Reserved {
    Back,
    Next,
    Quit,
    Help,
}

/// Map a line to a reserved command, or `None` for a real answer. Only the
/// colon-prefixed forms are commands, so "back" typed as a value is left alone.
fn reserved(line: &str) -> Option<Reserved> {
    match line.trim().to_ascii_lowercase().as_str() {
        ":back" | ":b" => Some(Reserved::Back),
        ":next" | ":n" => Some(Reserved::Next),
        ":quit" | ":q" => Some(Reserved::Quit),
        ":help" | ":h" | ":?" => Some(Reserved::Help),
        _ => None,
    }
}

fn parse_bool(s: &str, default: bool) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "" => Some(default),
        "y" | "yes" | "true" | "1" => Some(true),
        "n" | "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

/// Colour is on only when someone is there to see it and nothing has asked it
/// off: a TTY on both ends, `NO_COLOR` unset, and `TERM` not `dumb`. On Windows,
/// legacy consoles do not read ANSI, so it is gated on a marker that only the
/// modern terminals set (`WT_SESSION` for Windows Terminal, or an explicit
/// `TERM` as set by Git Bash and friends).
fn color_enabled(interactive: bool) -> bool {
    if !interactive || std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
        return false;
    }
    if cfg!(windows) {
        return std::env::var_os("WT_SESSION").is_some() || std::env::var_os("TERM").is_some();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colon_forms_are_commands_and_bare_words_are_not() {
        assert!(matches!(reserved(":back"), Some(Reserved::Back)));
        assert!(matches!(reserved(":Q"), Some(Reserved::Quit)));
        assert!(matches!(reserved(":help"), Some(Reserved::Help)));
        // A goal literally named "back" must survive as an answer.
        assert!(reserved("back").is_none());
        assert!(reserved("quit the app").is_none());
    }

    #[test]
    fn bool_parsing_covers_the_spellings_people_use() {
        assert_eq!(parse_bool("y", false), Some(true));
        assert_eq!(parse_bool("NO", true), Some(false));
        assert_eq!(parse_bool("", true), Some(true));
        assert_eq!(parse_bool("maybe", true), None);
    }

    #[test]
    fn no_color_env_forces_plain_output() {
        // Whatever the terminal is, NO_COLOR wins. Checked through the pure
        // helper so the test does not depend on the test runner's TTY.
        std::env::set_var("NO_COLOR", "1");
        assert!(!color_enabled(true));
        std::env::remove_var("NO_COLOR");
    }
}
