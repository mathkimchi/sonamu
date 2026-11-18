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
};

const SYSTEM: &str = "You are the assistant in Sonamu, a Wayland compositor. \
    You only see this chat. Answer briefly, in plain text (no Markdown).";

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
    let url = format!(
        "{}/v1/chat/completions",
        sonamu_common::settings::get()
            .agent_url
            .trim_end_matches('/')
    );
    thread::spawn(move || {
        let result = stream(&url, &chat, &cancel, |text| {
            let _ = sender.send(Streamed::Text(text));
            notify();
        });
        let _ = sender.send(Streamed::Done(result));
        notify();
    });
    receiver
}

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
    let response = ureq::post(url)
        .header("content-type", "application/json")
        .send(body.to_string())
        .map_err(|err| format!("agent at {url}: {err}"))?;
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
