//! The buffer line's agent (#155): a chatbot on an OpenAI-compatible server (setting `agent_url`), streamed

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const SYSTEM: &str = "You are the assistant in Sonamu, a Wayland compositor. \
    You only see this chat. Answer briefly, in plain text (no Markdown).";

/// What else the buffer line runs, for when there's no agent
const OTHERS: &str = "`$` runs shell, `!` runs Luau";

/// How long `check` waits for the server
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// The server's base URL (setting `agent_url`), else why there's none
fn url() -> Result<String, String> {
    match sonamu_common::settings::get().agent_url {
        Some(url) => Ok(url.trim_end_matches('/').to_string()),
        None => Err(format!(
            "No agent: setting `agent_url` isn't set (eg llama.cpp's `llama-server`). {OTHERS}"
        )),
    }
}

/// On a thread: a warning if there's no agent or it doesn't answer, `notify`ing once
pub(crate) fn check(notify: impl Fn() + Send + 'static) -> mpsc::Receiver<Option<String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let warning = match url() {
            Ok(url) => ureq::get(format!("{url}/v1/models"))
                .config()
                .timeout_global(Some(CHECK_TIMEOUT))
                .build()
                .call()
                .err()
                .map(|err| format!("No agent at {url} ({err}). {OTHERS}")),
            Err(err) => Some(err),
        };
        let _ = sender.send(warning);
        notify();
    });
    receiver
}

/// A message of the chat so far
pub(crate) struct Message {
    /// `user` or `assistant`
    pub role: &'static str,
    pub content: String,
}

/// What a reply sends while it streams
pub(crate) enum Streamed {
    Text(String),
    Done(Result<(), String>),
}

/// Asks the agent on a thread; its reply comes in pieces, `notify`ing each; `cancel` stops it
pub(crate) fn ask(
    chat: Vec<Message>,
    cancel: Arc<AtomicBool>,
    notify: impl Fn() + Send + 'static,
) -> mpsc::Receiver<Streamed> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = url().and_then(|url| {
            stream(&url, &chat, &cancel, |text| {
                let _ = sender.send(Streamed::Text(text));
                notify();
            })
        });
        let _ = sender.send(Streamed::Done(result));
        notify();
    });
    receiver
}

/// The reply from the server at `url`
fn stream(
    url: &str,
    chat: &[Message],
    cancel: &AtomicBool,
    mut on_text: impl FnMut(String),
) -> Result<(), String> {
    let messages: Vec<Value> = std::iter::once(json!({"role": "system", "content": SYSTEM}))
        .chain(
            chat.iter()
                .map(|message| json!({"role": message.role, "content": message.content})),
        )
        .collect();
    let body = json!({
        "messages": messages,
        "stream": true,
        // Qwen3's thinking takes long and isn't shown
        "chat_template_kwargs": {"enable_thinking": false},
    });
    let response = ureq::post(format!("{url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .send(body.to_string())
        .map_err(|err| format!("No agent at {url} ({err}). {OTHERS}"))?;
    // server-sent events: `data: {chunk}` lines, then `data: [DONE]`
    for line in BufReader::new(response.into_body().into_reader()).lines() {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let line = line.map_err(|err| err.to_string())?;
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        let chunk: Value = serde_json::from_str(data).map_err(|err| err.to_string())?;
        if let Some(text) = chunk["choices"][0]["delta"]["content"].as_str() {
            on_text(text.to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs llama.cpp's `llama-server` at 127.0.0.1:11434"]
    fn agent_replies() {
        let chat = [Message {
            role: "user",
            content: "Reply with just the word: pong".to_string(),
        }];
        let mut text = String::new();
        stream(
            "http://127.0.0.1:11434",
            &chat,
            &AtomicBool::new(false),
            |piece| text.push_str(&piece),
        )
        .unwrap();
        assert!(text.to_lowercase().contains("pong"), "{text}");
    }
}
