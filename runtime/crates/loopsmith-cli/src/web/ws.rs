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

use crate::web::exec::{JobState, Jobs};
use axum::extract::ws::{Message, WebSocket};
use serde_json::json;

pub async fn pump(mut socket: WebSocket, jobs: Jobs, id: String) {
    // Subscribe before replaying. The other order drops any line printed
    // between the replay and the subscription, which is the classic way to
    // lose exactly the line that mattered.
    let Some(mut rx) = jobs.subscribe(&id) else {
        let _ = socket
            .send(Message::Text(
                json!({ "type": "error", "message": "no such job" })
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    };

    for line in jobs.lines(&id) {
        if socket
            .send(Message::Text(
                json!({ "type": "line", "line": line }).to_string().into(),
            ))
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

    loop {
        match rx.recv().await {
            Ok(line) => {
                if socket
                    .send(Message::Text(
                        json!({ "type": "line", "line": line }).to_string().into(),
                    ))
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
