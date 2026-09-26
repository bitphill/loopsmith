//! The live run console's socket.
//!
//! One socket per job. It replays the retained buffer first and then follows
//! the broadcast channel, so a browser that opens the console late still reads
//! the run from the beginning — the alternative is joining a conversation
//! mid-sentence, which is exactly the wrong experience for someone trying to
//! work out why a run stalled.
//!
//! The transport is `axum::extract::ws`, which is RFC6455 over the server this
//! process already runs. The browser side is a plain `new WebSocket(...)`.

use crate::exec::{JobLine, JobState, Jobs};
use axum::extract::ws::{Message, WebSocket};
use serde_json::json;

/// One line, plus whatever the run log said in it.
///
/// Every line still arrives verbatim — the console is the record and nothing
/// is filtered out of it. `event` is the same line read by
/// [`crate::progress`], present only where the line was a ledger entry, and
/// it is what the run view is drawn from. Parsing here rather than in the
/// browser keeps the one copy of the format on the side that owns it.
fn line_message(line: &JobLine) -> String {
    json!({
        "type": "line",
        "line": line,
        "event": crate::progress::parse(&line.text),
    })
    .to_string()
}

pub async fn pump(mut socket: WebSocket, jobs: Jobs, id: String) {
    // Subscribe before replaying. The other order drops any line printed
    // between the replay and the subscription, which is the classic way to
    // lose exactly the line that mattered.
    //
    // Nothing to subscribe to is not an error: a job that has already finished
    // has no sender, and its retained lines are the whole story. Only a job
    // that was never here is.
    let rx = jobs.subscribe(&id);
    if jobs.summary(&id).is_none() {
        let _ = socket
            .send(Message::Text(
                json!({ "type": "error", "message": "no such job" })
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    }

    for line in jobs.lines(&id) {
        if socket
            .send(Message::Text(line_message(&line).into()))
            .await
            .is_err()
        {
            return;
        }
    }

    if let Some(summary) = jobs.summary(&id) {
        let _ = socket
            .send(Message::Text(
                json!({ "type": "state", "summary": summary })
                    .to_string()
                    .into(),
            ))
            .await;
        if summary.state != JobState::Running {
            // Already finished: the replay was the whole story.
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    }

    // Still going, so follow it. The loop ends when the job's sender is
    // dropped, which `Jobs::finish` does after the last line — and the state
    // message below is then the one that tells the console it is over.
    if let Some(mut rx) = rx {
        follow(&mut socket, &mut rx).await;
    }

    if let Some(summary) = jobs.summary(&id) {
        let _ = socket
            .send(Message::Text(
                json!({ "type": "state", "summary": summary })
                    .to_string()
                    .into(),
            ))
            .await;
    }
    let _ = socket.send(Message::Close(None)).await;
}

/// Follow a job until its sender is dropped.
///
/// Its own function so that the socket's closing message — the one that tells
/// the console the run ended — is written once, on the path out, rather than
/// once for a job that had already finished and again for one that finished
/// while it was being watched. Getting only the second of those right is how
/// a finished run sat there saying `running`.
async fn follow(
    socket: &mut WebSocket,
    rx: &mut tokio::sync::broadcast::Receiver<crate::exec::JobLine>,
) {
    loop {
        match rx.recv().await {
            Ok(line) => {
                if socket
                    .send(Message::Text(line_message(&line).into()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            // Lagged: this consumer fell behind a very chatty run. Say so
            // rather than silently skipping lines, so nobody debugs a gap that
            // was never in the output.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                let _ = socket
                    .send(Message::Text(
                        json!({
                            "type": "lagged",
                            "skipped": n,
                            "message": format!("{n} line(s) skipped — output arrived faster than this page could read it")
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;
            }
            // Sender gone: the job finished and its channel was dropped.
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
