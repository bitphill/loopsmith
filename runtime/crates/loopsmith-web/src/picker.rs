//! The operating system's own folder chooser.
//!
//! Typing an absolute path from memory is a bad ask, and the browser cannot
//! help: `showDirectoryPicker()` hands back an opaque handle with no filesystem
//! path, deliberately, so it is useless for telling a CLI where to work.
//!
//! But the server is a local process on the very machine the user is sitting
//! at, which means it can just ask the OS. Each platform ships a folder dialog
//! reachable from a command:
//!
//! | Platform | Dialog |
//! |---|---|
//! | macOS | `osascript` → AppleScript `choose folder` |
//! | Windows | PowerShell → `FolderBrowserDialog` |
//! | Linux | `zenity`, else `kdialog`, else nothing |
//!
//! Linux is the one that can genuinely have neither, so the failure is reported
//! as "type the path instead" rather than as an error — the text box has always
//! worked and still does.

use std::time::Duration;

/// Long enough for someone to actually browse to a folder, short enough that a
/// dialog which somehow never appears does not pin a thread forever.
const DIALOG_TIMEOUT: Duration = Duration::from_secs(300);

pub struct Picked {
    pub path: Option<String>,
    /// Set when no dialog could be shown at all, in words the UI can print.
    pub unavailable: Option<String>,
}

/// Which dialog this machine can show, if any.
pub fn available() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        loopsmith_util::which("osascript").map(|_| "macOS")
    } else if cfg!(target_os = "windows") {
        loopsmith_util::which("powershell").map(|_| "Windows")
    } else {
        loopsmith_util::which("zenity")
            .map(|_| "zenity")
            .or_else(|| loopsmith_util::which("kdialog").map(|_| "kdialog"))
    }
}

/// Show the dialog and wait. `start_in` seeds the folder it opens at.
pub async fn choose(start_in: Option<&str>) -> Picked {
    let Some(kind) = available() else {
        return Picked {
            path: None,
            unavailable: Some(
                "This machine has no folder dialog loopsmith can open. On Linux, \
                 installing `zenity` or `kdialog` adds one. Until then, type the \
                 path into the box."
                    .into(),
            ),
        };
    };

    let out = match kind {
        "macOS" => {
            // `choose folder` returns an alias; POSIX path turns it into
            // something a CLI can use. The default-location clause is omitted
            // entirely when there is no usable starting point, because an
            // AppleScript alias to a non-existent path is a runtime error
            // rather than a fallback.
            let script = match start_in.filter(|p| std::path::Path::new(p).is_dir()) {
                Some(dir) => format!(
                    "POSIX path of (choose folder with prompt \"Choose a folder for this loop\" \
                     default location POSIX file \"{}\")",
                    dir.replace('\\', "\\\\").replace('"', "\\\"")
                ),
                None => "POSIX path of (choose folder with prompt \"Choose a folder for this loop\")"
                    .to_string(),
            };
            run("osascript", &["-e", &script]).await
        }
        "Windows" => {
            let script = "Add-Type -AssemblyName System.Windows.Forms; \
                 $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
                 $d.Description = 'Choose a folder for this loop'; \
                 if ($d.ShowDialog() -eq 'OK') { Write-Output $d.SelectedPath }";
            run("powershell", &["-NoProfile", "-STA", "-Command", script]).await
        }
        "zenity" => {
            run(
                "zenity",
                &["--file-selection", "--directory", "--title=Choose a folder for this loop"],
            )
            .await
        }
        _ => run("kdialog", &["--getexistingdirectory", start_in.unwrap_or(".")]).await,
    };

    match out {
        Ok(text) => {
            let path = text.trim().trim_end_matches('/').to_string();
            Picked {
                // Empty output is a cancelled dialog, which is a normal answer
                // and not a failure worth reporting to the user.
                path: (!path.is_empty()).then_some(path),
                unavailable: None,
            }
        }
        Err(e) => Picked {
            path: None,
            unavailable: Some(e),
        },
    }
}

async fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = tokio::time::timeout(
        DIALOG_TIMEOUT,
        tokio::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| "the folder dialog was left open too long".to_string())?
    .map_err(|e| format!("could not open the folder dialog: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    // A cancelled dialog exits non-zero on every platform here, with nothing on
    // stdout. That is the user saying no, so it is success with no path.
    if out.status.success() || stdout.trim().is_empty() {
        return Ok(stdout);
    }
    Err(format!(
        "the folder dialog failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_reports_whether_it_has_a_dialog_at_all() {
        // Not asserting which: the point is that the answer is a definite
        // yes-or-no the UI can act on, rather than a guess that fails later.
        let kind = available();
        if cfg!(target_os = "macos") {
            assert_eq!(kind, Some("macOS"), "macOS always ships osascript");
        }
    }

    #[tokio::test]
    async fn a_machine_with_no_dialog_says_to_type_the_path_instead() {
        // Only meaningful where there genuinely is none; elsewhere it would
        // open a real dialog and block a test run forever, so this asserts the
        // shape of the refusal rather than calling `choose`.
        if available().is_none() {
            let picked = choose(None).await;
            assert!(picked.path.is_none());
            assert!(picked.unavailable.unwrap().contains("type the path"));
        }
    }
}
