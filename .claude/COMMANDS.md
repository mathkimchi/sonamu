# Commands and Luau

Emacs-like: every action is a named command; keybinds, hub, buffer line and socket call the same ones. Luau via `mlua` (sandboxed).

## Core (`sonamu_common::command`)

- `NodeUid` (global `AtomicU64`), registry uid → (world sender, `NodeId`). Registered on insert, dropped on close/unwrap; carried in `DetachedTree::uid`; not saved. Mains learn theirs via `StandardEvent::NodeUid` (`AppletContext::uid`).
- `NodeCall { uid, name, args, reply }` on the owning world's channel → `NodeHolder::handle_node_call` (root: `RootNodeApplet::handle_node_call`; projects: `StandardApplet::handle_node_call`). Tree commands (`focus`, `root`, `spawn`, `open`, `quit`, `reload_settings`), node commands, else `StandardEvent::Command` to the applet (`StandardApplet::command`, sync, `Value` in/out).
- Signatures: `AppletSpawnerTrait::commands` → `command::declare`; checked in the node's world (`command::check`). Clashes with `NODE_COMMANDS` dropped.
- Type: derived when asked (`NodeTree::applet_type`), not stored. Right after `add_child` it's `applet` until the first publish.
- Effects `read < modify < destructive < spawn` (`command::Effect`); limited evals (`eval_limited`, `sonamu-msg eval --max`) run in their own env and builtins check `allow`. Everything else unlimited.
- Keybinds: from setting `keybinds` (`.claude/SETTINGS.md`); `standard_keybinds::standard_keybind` → `KeybindCommand`, sent to the applet's own node (`command::send`). Rust, so a stuck script can't block them.

## Script thread (`sonamu_script`)

- One VM, one shared env; one job at a time (evals, completions, bindings, hooks queue). Calls block the script thread; worlds never wait on it. Ctrl+C interrupts (`interrupt`).
- `start()` runs `<config dir>/init.luau` (again after `sonamu.reset()`). `require("./x")` relative, per-module globals reading through.
- `keymap.set({mods}, key, fn, {type}?)`: static `BINDINGS`; `type`-scoped wins (focus type lags: `FOCUS_TYPE`). Runner checks after settings keybinds.
- `events.on`: `focus(node)`, `window(app_id)`, `open(node)`, `close(node)`, `settings()`, `screens(count)`. `subscribe` streams them.
- Messages: `sonamu_script::message` (last 200, `sonamu.messages()`), echoed over the buffer line row 5 s.
- `complete`: Tab completion; runs read-only getters only. Right after a call, the separator (`:` node, `.` table).
- `sonamu.definitions()`: luau-lsp definitions (unchecked).

## API

- `Node`: `close`, `focus`, `add_child(applet, args?)`, `swap_sibling`, `toggle_half`, `wrap_project`, `parent`, `children`, `inner` (project tree root), `type`, `title`, `cast(name)`; unknown keys → applet command. `<type>.from_node(n)`.
- `context.focus()`, `context.root()`; `process.spawn`, `process.output(prog, args?, dir?, fn?)`; `clipboard.copy/paste`; `sonamu.open/reload_settings/quit/reset/bar_layout`; `sonamu.traverse(kind, n?)` (root command, like the keybind); `help(name?)` (`describe`: tools, `HELP` lines, node methods, applet commands); `sonamu.complete(text)` (signed completions); `sonamu.config` (`Settings` userdata): settings as fields, eg `sonamu.config.border.width = 5` (`set_setting`: edits `settings.json` via `settings_file` like the settings editor: checked, defaults removed; the watcher reloads; fails for a settings program); objects are nested `Settings`, `tostring` gives JSON; Tab lists the defaults' and user's keys; `sonamu.definitions()` types it from the defaults.
- Applets: `CommandHub` `clear/set_title/open`; `file_manager` `root/selected/open`; `code_editor` `path/text/save`; `wl_surface` `app_id/pid`.

## Prompts

- `command_hub/repl.rs`: `Repl` shared by the hub and the buffer line (Up/Down recall shared, `RECALL`). A line's first char picks its `Kind`: `!` Luau, `$` shell (`shell.rs`, `sh -c` in its own process group, stdout+stderr streamed; Ctrl+C kills the group), else the default (hub: Luau; buffer line: agent). Tab: Luau on the script thread; shell `shell::complete` (first word: `$PATH` executables, else paths; `~/`; hidden only after `.`).
- Agent (#155, `command_hub/agent.rs`): streamed from an OpenAI-compatible `/v1/chat/completions` at setting `agent_url` (default `null`: none; the example config's is `llama-server` at `127.0.0.1:11434`) with `tools` (llama-server's Jinja template does Qwen's `<tool_call>`); system prompt: setting `agent_prompt`. Qwen3 thinking off. Tools: `sonamu_script::tools` (built in, not MCP): `help(name?)`, `complete(text)` (signed), `run_lua`, and always available functions on the focused node only (`title`, `type`, `setting`, `set_setting`, `traverse`, `swap_sibling`, `toggle_half`, `wrap_project`, `add_child`, `open`, `close`, `spawn`, `spawn_sibling`); applet commands only via `run_lua`. `applet` params' descriptions (sent with every request, and in `help`) list the declared applet types (`command::all_signatures`, but `wl_surface`; fixed after startup, so the prompt cache holds). Each call shows as `name(args)` and the prompt row asks `Run this? y/n` (`Repl::asking`; y/Enter, n/Esc, Ctrl+C refuses; buffer mode's Esc doesn't leave meanwhile); the result goes back to the model, at most `MAX_TOOL_ROUNDS` a reply. Later turns see each exchange's messages (`Entry::messages`: replies, `tool_calls`, `tool` results). Entering buffer mode runs `agent::check` (unset, or `/v1/models` fails within 2 s): a red `Repl::warning` until the next key; prompting without an agent errors; both point to `$` and `!`. Its chat is the history's finished agent entries (last `MAX_CHAT_ENTRIES`), so the hub's `clear` resets it. Tests `agent_replies`, `agent_calls_tools` are `#[ignore]`d (need the server at `127.0.0.1:11434`). Don't call `sonamu_script::interrupt` (eg Ctrl+C) in tests: it breaks others running in parallel.
- `KEY_CAPTURE` (count of `Capture`s, eg the settings editor recording a keybind): the runner skips settings, Luau and `pluck_place` bindings, `is_keybind` is true and the root skips its own, so every key reaches the focused applet. Ended (`end_capture`) when a wl client has the keyboard (runner) or focus leaves where it started (`RootNodeApplet::check_capture`): applets don't get `FocusChanged`.
- Buffer line (`RootNodeApplet::{buffer_mode, repl}`): keybind `buffer_mode` (default Super+Esc); `BUFFER_MODE` sends every key to the root and shadows symbol bindings unless Super. Active screen's bottom. Setting `buffer_line`. PageUp/PageDown and mouse scrolls over it (`buffer_line_height`, `ScrollSteps`) scroll the whole history (`scroll_buffer`; `buffer_window` grows up from the last entry, then moves), with a `grid_scrollbar` while scrolled.
- Socket eval: `Message::Eval { code, max }`, JSON lines (`msg::EvalReply`), keepalives every 100 ms cancel it when the client leaves. Node JSON `{"type", "node"}`.

## Gaps

- Hibernated stand-ins keep their saved type, so their commands fail: `pcall`.
- Untested live: buffer line UI (agent/shell entries included), keybinds as commands, `keymap.set` keys (injected keys don't arrive headless).
- Later: MCP (outside servers' tools for the agent; Sonamu's stay built in), socket client libraries; more agent tools (shell, reading files); agent tools on relative/absolute node paths, not only focus (TODO in `tools.rs`).
- Agent/shell output (`printed`) isn't capped like results (`MAX_OUTPUT_LINES`); a shell command's background child holding the pipe keeps the entry running.
