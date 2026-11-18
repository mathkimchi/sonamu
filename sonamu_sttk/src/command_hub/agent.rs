//! The buffer line's agent (#155): a chatbot on an OpenAI-compatible server (setting `agent_url`), streamed; its
//! `run_luau` tool runs Luau once the user allows it

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
    To look at or change the desktop, call run_luau; the user allows or refuses each call. \
    Answer briefly, in plain text (no Markdown).";

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

/// A message of the chat so far
pub(crate) struct Message {
    /// `user` or `assistant`
    pub role: &'static str,
    pub content: String,
}

/// What a reply (or a shell command) sends while it runs
pub(crate) enum Streamed {
    Text(String),
    /// The agent wants to run this Luau: `true` allows it (dropped: refused)
    Ask(mpsc::Sender<bool>),
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
                &mut |code| {
                    let (answer, answered) = mpsc::channel();
                    send(Streamed::Text(format!("run_luau: {code}\n")));
                    send(Streamed::Ask(answer));
                    let allowed = answered.recv().unwrap_or(false);
                    let result = if allowed {
                        run_luau(code)
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

/// The reply from the server at `url`, `on_text` as it streams; its Luau goes through `run`, which gives what to tell
/// the model
fn converse(
    url: &str,
    chat: &[Message],
    cancel: &AtomicBool,
    on_text: &mut dyn FnMut(String),
    run: &mut dyn FnMut(&str) -> String,
) -> Result<(), String> {
    let mut messages: Vec<Value> = std::iter::once(json!({"role": "system", "content": SYSTEM}))
        .chain(
            chat.iter()
                .map(|message| json!({"role": message.role, "content": message.content})),
        )
        .collect();
    let tools = json!([{
        "type": "function",
        "function": {
            "name": "run_luau",
            "description": format!(
                "Run Luau in Sonamu (after the user allows it); gives its prints and return values. API:\n{}",
                sonamu_script::help()
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "code": {"type": "string", "description": "eg `return context.focus():title()`"},
                },
                "required": ["code"],
            },
        },
    }]);
    for _ in 0..MAX_TOOL_ROUNDS {
        let (content, calls) = stream(url, &messages, &tools, cancel, on_text)?;
        if calls.is_empty() {
            return Ok(());
        }
        messages.push(json!({"role": "assistant", "content": content, "tool_calls": calls}));
        for call in calls {
            let arguments: Value =
                serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or_default())
                    .unwrap_or_default();
            let result = match arguments["code"].as_str() {
                Some(code) if call["function"]["name"] == "run_luau" => run(code),
                _ => "No such tool, or no `code`.".to_string(),
            };
            messages.push(json!({"role": "tool", "tool_call_id": call["id"], "content": result}));
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

/// Runs `code` on the script thread, waiting: its prints and return values, or its error
fn run_luau(code: &str) -> String {
    let (_handle, outputs) = sonamu_script::eval(code.to_string(), || {});
    let mut text = String::new();
    for output in outputs {
        match output {
            sonamu_script::Output::Print(line) => {
                text.push_str(&line);
                text.push('\n');
            }
            sonamu_script::Output::Done(Ok(values)) => {
                let values: Vec<String> = values.iter().map(ToString::to_string).collect();
                text.push_str(&values.join(", "));
                break;
            }
            sonamu_script::Output::Done(Err(err)) => {
                text = format!("{text}error: {err}");
                break;
            }
        }
    }
    if text.is_empty() {
        "(nothing)".to_string()
    } else {
        text
    }
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
        converse(
            "http://127.0.0.1:11434",
            &chat,
            &AtomicBool::new(false),
            &mut |piece| text.push_str(&piece),
            &mut |_| unreachable!(),
        )
        .unwrap();
        assert!(text.to_lowercase().contains("pong"), "{text}");
    }

    #[test]
    #[ignore = "needs llama.cpp's `llama-server` at 127.0.0.1:11434"]
    fn agent_runs_luau() {
        let chat = [Message {
            role: "user",
            content: "What's the focused window's title?".to_string(),
        }];
        let mut ran = Vec::new();
        let mut text = String::new();
        converse(
            "http://127.0.0.1:11434",
            &chat,
            &AtomicBool::new(false),
            &mut |piece| text.push_str(&piece),
            &mut |code| {
                ran.push(code.to_string());
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
