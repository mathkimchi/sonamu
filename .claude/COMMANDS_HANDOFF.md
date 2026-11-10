# Handoff: commands and Luau scripting

State and design: `.claude/COMMANDS.md` (Status, Decisions, v0 API, To do). This file: what to fix next and how.
Everything below is committed on `dev` (`b2b9bed` plan … `292682d`).

## Orientation

- Crates: `sonamu_common/sonamu_command` (uids, `Value`, `NodeCall`, registry, signatures, `call`/`send`),
  `sonamu_script` (Luau thread: `eval`, `complete`, `start`, `binding`, `emit`), `sonamu_sttk/src/command_hub/`
  (`repl.rs` shared prompt; hub), root buffer line in `root_node_applet.rs` (`buffer_mode`, `buffer_line`),
  `NodeHolder::handle_node_call` (tree + node commands), `sonamu_sar/src/msg_socket.rs` (`answer_eval`),
  `sonamu_common/src/clipboard.rs` + `SmithayState::{write_clipboard, set_clipboard}`.
- Build: `cargo` from `/run/current-system/sw/bin` (rustup's toolchain once pointed at a GC'd glibc). Luau builds C++
  via `mlua`'s vendored feature (slow first build).
- Checks used: `cargo clippy --workspace --all-targets` (pedantic, kept clean), `cargo test --workspace`
  (`sonamu_script` tests share one Luau state and a fake world; `settings::tests::schema_is_current` rewrites
  `assets/settings.schema.json` and fails once when settings change: rerun).
- Nothing was run in the live compositor: headless sway (`.claude/TESTING.md`) wasn't installed. First job: run it.

## Fix first (by risk)

2. **Unmodified bindings steal buffer-line typing.** `AppletRunner::handle_ui_event` matches settings keybinds and
   `sonamu_script::binding` before the root sees the key. Skip bindings without mods (or all but media keys) while
   `root_node_applet::BUFFER_MODE` is set. That static is in sttk, which sar doesn't depend on: move it to
   `sonamu_common` (or `sonamu_command`) so the runner can read it.
3. **Errors only on stderr** for init/bindings/hooks. Route them to the buffer line: keep a small "messages" list
   in `Repl` (or a channel the root drains) and show the last one above the prompt / as a notification
   (`sonamu_common::notification`).
4. **One queue.** A slow `sonamu-msg eval` / `process.output` delays everything. Options: run `process.output` off
   the script thread (Luau can't yield without `mlua` async; would need coroutines + `async` feature), or accept and
   document. Measure first.

## Gaps (see COMMANDS.md To do for the full list)

- Signatures unchecked: parse `Signature::params` (`name: type`, `T?`, `{T}`) once and check arity/types in
  `sonamu_script::call` before sending; errors name the param.
- Tables: `from_lua` drops map keys. Add `Value::Map(BTreeMap<String, Value>)` (serde untagged works), convert both
  ways, `Display` as `{k = v}`.
- `cast`/typed handles: `cast` could return a userdata carrying the type tag so `definitions()` types line up;
  `file_manager.from_node` = a table per declared type with `from_node`.
- Socket node JSON: serialize `Value::Node` as `{"type": .., "node": uid}` (needs the type: ask the world, or store
  the tag in the registry, which also closes "type tag derived on demand").
- Project contents: add a node command `inner(): Node?` answered by applets holding a world (like `INNER_FOCUS`,
  returning their inner root's uid); `children()` of a project node could then stay outer.
- `hub:run_output`: reimplement on `process.output` semantics (worker thread + reply later). `StandardApplet::command`
  is sync, so either add `Answer::Later`-style replies for commands or drop `run_output` for `process.output`.
- Buffer line: scrolling its output (PageUp/Down in buffer mode), drawing on every screen, sharing history with hubs
  (one `Repl` history store), a newline key (Shift+Enter) and type-ahead.
- Events: emit `close`, `open`, `settings`, `screens`; pass the focused node to `focus`; forward over the socket as
  `EvalReply`-like lines (a `sonamu-msg subscribe`).
- `keymap.unset`, and focus-path layering / `when` conditions (see the plan's Later).
- `init.luau` environments: per-file environments + `require` (Roblox model) once there are several files.
- Clipboard: a copy key in the hub/buffer line (last output) and the code editor (selection, once it has one);
  primary selection (`primary_selection` state is there in smithay).
- Effect levels on commands (read / modify / destructive / spawn) before an MCP bridge; `process.*` runs anything.
- `docs/usage.md`: mention Super+Esc and the buffer line (agents here may not edit `docs/`; ask the user).

## Conventions kept

- One commit per change, message + `Co-Authored-By`/`Claude-Session` trailers; `timeout 20 git commit … ||
  git commit --no-gpg-sign …`.
- Update `.claude/COMMANDS.md` (and `SETTINGS.md`, `CLAUDE.md` pointers) with each change; terse.
- Small, asked-for changes; report deviations from COMMANDS.md's Decisions in its "Deviations" list.
