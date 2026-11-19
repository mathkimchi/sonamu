//! The buffer line's agent (#155): a chatbot on an OpenAI-compatible server (setting `agent_url`), streamed; its
//! tools (`sonamu_script::tools`) run once the user allows each call

use serde_json::{Value, json};
use sonamu_script::tools;
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

/// Tool calls in one reply at most, so a confused model can't loop forever
const MAX_TOOL_ROUNDS: usize = 8;

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

/// What a reply (or a shell command) sends while it runs
pub(crate) enum Streamed {
    Text(String),
    /// The agent wants to run its tool call: `true` allows it (dropped: refused)
    Ask(mpsc::Sender<bool>),
    /// A message of the agent's exchange (its replies, tool calls and results), for later turns
    Message(Value),
    Done(Result<(), String>),
}

/// Asks the agent on a thread, after the `chat` so far (OpenAI-style messages); its reply comes in pieces,
/// `notify`ing each; `cancel` stops it
pub(crate) fn ask(
    chat: Vec<Value>,
    cancel: Arc<AtomicBool>,
    notify: impl Fn() + Send + 'static,
) -> mpsc::Receiver<Streamed> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let send = |streamed| {
            let _ = sender.send(streamed);
            notify();
        };
        let result = url().and_then(|url| {
            converse(
                &url,
                &chat,
                &cancel,
                &mut |text| send(Streamed::Text(text)),
                &mut |message| send(Streamed::Message(message)),
                &mut |name, args| {
                    let (answer, answered) = mpsc::channel();
                    send(Streamed::Text(format!("{}\n", tools::shown(name, args))));
                    send(Streamed::Ask(answer));
                    let allowed = answered.recv().unwrap_or(false);
                    let result = if allowed {
                        tools::call(name, args, &|| cancel.load(Ordering::Relaxed))
                    } else {
                        "The user refused to run it.".to_string()
                    };
                    send(Streamed::Text(format!("→ {result}\n")));
                    result
                },
            )
        });
        send(Streamed::Done(result));
    });
    receiver
}

/// The reply from the server at `url` after `chat`, `on_text` as it streams and `on_message` for each of its
/// messages; its tool calls go through `run`, which gives what to tell the model
fn converse(
    url: &str,
    chat: &[Value],
    cancel: &AtomicBool,
    on_text: &mut dyn FnMut(String),
    on_message: &mut dyn FnMut(Value),
    run: &mut dyn FnMut(&str, &Value) -> String,
) -> Result<(), String> {
    let system = sonamu_common::settings::get().agent_prompt;
    let mut messages: Vec<Value> = std::iter::once(json!({"role": "system", "content": system}))
        .chain(chat.iter().cloned())
        .collect();
    let mut add = |messages: &mut Vec<Value>, message: Value| {
        on_message(message.clone());
        messages.push(message);
    };
    let tools = tools::definitions();
    for _ in 0..MAX_TOOL_ROUNDS {
        let (content, calls) = stream(url, &messages, &tools, cancel, on_text)?;
        if calls.is_empty() {
            add(
                &mut messages,
                json!({"role": "assistant", "content": content}),
            );
            return Ok(());
        }
        add(
            &mut messages,
            json!({"role": "assistant", "content": content, "tool_calls": calls}),
        );
        for call in calls {
            let arguments: Value =
                serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or_default())
                    .unwrap_or_default();
            let name = call["function"]["name"].as_str().unwrap_or_default();
            let result = run(name, &arguments);
            add(
                &mut messages,
                json!({"role": "tool", "tool_call_id": call["id"], "content": result}),
            );
        }
    }
    Err("the agent kept calling tools".to_string())
}

/// One reply of the server: its text (also `on_text` as it streams) and tool calls
fn stream(
    url: &str,
    messages: &[Value],
    tools: &Value,
    cancel: &AtomicBool,
    on_text: &mut dyn FnMut(String),
) -> Result<(String, Vec<Value>), String> {
    let body = json!({
        "messages": messages,
        "tools": tools,
        "stream": true,
        // Qwen3's thinking takes long and isn't shown
        "chat_template_kwargs": {"enable_thinking": false},
    });
    let response = ureq::post(format!("{url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .send(body.to_string())
        .map_err(|err| format!("No agent at {url} ({err}). {OTHERS}"))?;
    let mut content = String::new();
    // each streams in pieces, by `index`: `id` and `name` first, then `arguments`
    let mut calls: Vec<Value> = Vec::new();
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
        let delta = &chunk["choices"][0]["delta"];
        if let Some(text) = delta["content"].as_str() {
            content.push_str(text);
            on_text(text.to_string());
        }
        for piece in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = piece["index"].as_u64().unwrap_or_default() as usize;
            if calls.len() <= index {
                calls.resize_with(index + 1, || {
                    json!({"type": "function", "id": "", "function": {"name": "", "arguments": ""}})
                });
            }
            let call = &mut calls[index];
            if let Some(id) = piece["id"].as_str() {
                call["id"] = id.into();
            }
            for field in ["name", "arguments"] {
                if let Some(text) = piece["function"][field].as_str() {
                    let joined = format!(
                        "{}{text}",
                        call["function"][field].as_str().unwrap_or_default()
                    );
                    call["function"][field] = joined.into();
                }
            }
        }
    }
    Ok((content, calls))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs llama.cpp's `llama-server` at 127.0.0.1:11434"]
    fn agent_replies() {
        let chat = [json!({"role": "user", "content": "Reply with just the word: pong"})];
        let mut text = String::new();
        converse(
            "http://127.0.0.1:11434",
            &chat,
            &AtomicBool::new(false),
            &mut |piece| text.push_str(&piece),
            &mut |_| {},
            &mut |_, _| unreachable!(),
        )
        .unwrap();
        assert!(text.to_lowercase().contains("pong"), "{text}");
    }

    #[test]
    #[ignore = "needs llama.cpp's `llama-server` at 127.0.0.1:11434"]
    fn agent_calls_tools() {
        let chat = [json!({"role": "user", "content": "What's the focused window's title?"})];
        let mut ran = Vec::new();
        let mut text = String::new();
        converse(
            "http://127.0.0.1:11434",
            &chat,
            &AtomicBool::new(false),
            &mut |piece| text.push_str(&piece),
            &mut |_| {},
            &mut |name, args| {
                ran.push(tools::shown(name, args));
                "Firefox".to_string()
            },
        )
        .unwrap();
        assert!(
            !ran.is_empty() && text.contains("Firefox"),
            "{ran:?} {text}"
        );
    }
}
