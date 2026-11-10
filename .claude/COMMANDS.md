# Commands and scripting (in progress)

Emacs-like model: every action is a named command; keybinds, the hub, the socket and (later) agents all call the same ones.
Embedded language: Luau via `mlua`. Other languages: over the socket.

## Status (milestones 1–5 done; see To do)

- `sonamu_common/sonamu_command` (re-exported as `sonamu_common::command`): `NodeUid`, `Value`, `NodeCall`, registry,
  `call` (blocks, polls `cancelled` every 50 ms), `focus` (descends through `INNER_FOCUS`), arg helpers.
- `NodeTree` nodes have a `uid`, registered on insert, unregistered on close/unwrap/drop, kept in `DetachedTree::uid`.
- `NodeHolder::handle_node_call`: tree commands (`focus`, `root`, `spawn`, `open`, `quit`, `reload_settings`) and node
  commands; others go to the applet as `StandardEvent::Command` (the `reply` sender rides in the event, no ids).
- Worlds: root (`RootNodeApplet::handle_node_call`, also `set_root_world`) and projects (`AppletContext::world`,
  `StandardApplet::handle_node_call`; `command(INNER_FOCUS)`).
- `sonamu_script`: script thread, sandboxed Luau, `print`, return values (`return <code>` tried first, like a REPL),
  `Node` userdata (node commands, `cast`, `__index` → applet command, `==`, `tostring`), cancel via interrupt.
- Hub: Luau only, one eval at a time (like a shell: no prompt, keys ignored but Ctrl+C/PageUp/PageDown, until it ends), `print`s then results (strings unquoted), Ctrl+C cancels; commands `clear`, `set_title`, `open` (replace the hub, as
  before; `"wl", {argv}` too), `run_output` (old `$`).
- Signatures: `AppletSpawnerTrait::commands` (`&'static [command::Signature]`: name, params, returns, doc), declared
  into `command::declare` when spawners register (`RootNodeApplet::new`, `register_applet_spawner`); clashes with
  `command::NODE_COMMANDS` dropped with a warning. Hub: `COMMANDS`. `sonamu.help()` (also global `help()`; a bare `help` in the hub runs it) lists them per type, and the hub keys.
- Tab: `sonamu_script::complete` (a job on the script thread, answered through the waker, dropped if the prompt
  changed): globals (incl. builtins), table fields after `.`, after `:` on a node its `Node` methods + its type's
  signatures. To know the node, completion runs read-only getters only: `context.focus()`, `context.root()`,
  `:parent()` (deviation: Q22 said no calls); `:add_child()` gives a node of unknown type (`Node` methods only).
- Paste: Ctrl+V / Ctrl+Shift+V (`sonamu_common::clipboard::paste`) inserts at the cursor; newlines kept (shown `↵`).

- Buffer line (root, `RootNodeApplet::{buffer_mode, repl}`): Super+Esc toggles buffer mode (like vim's normal
  mode); `sonamu_common::BUFFER_MODE` makes `is_keybind` send every key to the root then, and the runner
  (`runner::run_binding`) skip settings/`keymap.set` bindings on keys with a symbol unless Super is held (minibuffer-like
  shadowing; media/F-keys and Super combos still work). Esc clears the prompt, or leaves if empty.
  Bottom of the active screen: the last entry's output and Tab's candidates (≤ `BUFFER_OUTPUT_ROWS`) over the prompt.
  Setting `buffer_line` (`.claude/SETTINGS.md`): `always` reserves the row and shows `-- INSERT --` outside buffer mode.
  `context.focus()` there is the real focus (no "previous focus" problem). Not a node: no applet commands of its own.
- `command_hub/repl.rs`: `Repl` (prompt, history, recall, evals, completion, paste, `lines`, `draw_prompt`), shared by
  the hub and the buffer line; owners pass a `Waker` and call `wake`. Hub keeps scrolling, its commands and its window.
- The hub is no longer in the treeview's New menu or the project overview (`NewApplet::CommandHub`, `n` row gone);
  still `command_hub` (`sonamu.open`, `n:add_child`); Ctrl+Shift++ is gone too.

Try in the buffer line (Super+Esc) or a hub:

```lua
sonamu.help()
context.focus():title()                     -- in a hub, focus is the hub; in the buffer line, the real focus
for _, n in context.root():children() do print(n:type(), n:title()) end
context.focus():parent():add_child("file_manager", {"/tmp"})
context.focus():set_title("Lua hub")        -- in a hub: applet command, via __index
context.focus():run_output("ls", {"-a"})    -- in a hub
x = 5                                       -- then `x * 2` gives 10
while true do end                           -- Ctrl+C cancels
```

Deviations from the decisions below:
- Calls block the script thread instead of `mlua` async (same effect: worlds never wait on it).
- Type tag: derived when asked (`NodeTree::applet_type`: wl surface, else saved/hibernated spawner name, else
  `container`/`applet`), not stored (decided: it changes asynchronously, eg placeholders, hibernation, a publish).
- Signatures are checked in the node's world (`command::check` in `NodeHolder::handle_node_call`, before forwarding;
  arity and `string`/`number`/`boolean`/`{T}`/`T?`, other types just non-nil; undeclared commands pass, eg
  `INNER_FOCUS`), not in the script before sending.
- `cast` takes the type name (`n:cast("file_manager")`), returns the same `Node`; no per-type handles.
- No `split` (splits no longer exist). `parent()` of a project's inner root is the project node's parent (the two
  are one row): mains get `StandardEvent::NodeUid` (`NodeTree::register` and wherever a main is replaced; kept in
  `AppletContext::uid`), and `ProjectApplet::handle_node_call` asks its own node's world from a thread. The root
  tree's root has no parent (nil).
- `hub:run_output` blocks the hub's thread while the program runs; `process.output` (script thread, Ctrl+C kills it) doesn't.

- Socket: `Message::Eval(code)`; the runner answers from a thread (`msg_socket::answer_eval`) with JSON lines
  (`msg::EvalReply`: `print`, then `done` `Ok(values)`/`Err`), plus empty keepalive lines every 100 ms, which fail once
  the sender is gone and cancel the eval (reading can't tell: it already shut its writing half). `sonamu-msg eval
  '<code>'` (`-`: stdin): prints to stderr, one value as JSON (several as an array) on stdout, error → exit 1. A node
  is its uid number in JSON (not `{"type", "node"}`).

- Startup / keys / hooks (`sonamu_script`): `start()` (the root, at init) runs `<config dir>/init.luau` (again after
  `sonamu.reset()`, which also drops bindings). `keymap.set({mods}, key, fn)`: keys in the static `BINDINGS`
  (matched like settings `Keybind`s; same key replaces), fns in the state's registry; the runner checks
  `binding(..)` after the settings keybinds (before the root, so any focus) and queues `run_binding`.
  `events.on(event, fn)`; the root `emit`s `focus` (no args, `remember_focus` sees the path change) and `window`
  (app id, on `WlSurfaceRegistered`). These run as `Job::Call`: no one waits; their `print`s and errors are messages (below).
- Messages (Emacs' `*Messages*` + echo area): `sonamu_script::message` keeps the last 200 (`sonamu.messages()`), logs
  them to stderr, and calls `on_message` (the root's waker); the root (`echo_message`) shows the last one over the
  active screen's buffer line row for 5 s (errors in red; `Bar::None` when the row isn't reserved). Decided over: errors
  as desktop notifications (prints only in the list), or the last shown only when buffer mode next opens.
  Example: `examples/config/init.luau` (all commented).

- `sonamu.definitions()`: luau-lsp definitions (`declare class Node`, a class per declared applet type named like it,
  extending `Node`; the globals), eg `sonamu-msg eval 'sonamu.definitions()' > sonamu.d.luau`. Not checked against
  luau-lsp yet. `cast` still returns `Node`, so applet methods need an annotation (`local fm: file_manager = n :: any`).

- Keybinds: `standard_keybinds::standard_keybind` maps a key to a `KeybindCommand` (`quit`, `wrap_project`, `close`,
  `toggle_half`, `spawn(program)`, `swap_sibling(±1)`); `handle_standard_keybinds` sends it to the applet's own node
  (`command::send`, no answer; as the old `NodeRequest` via `KeybindCommand::request` until `AppletContext::uid` is
  known). `standard_keybind_request` (wl tiles, session placeholders, file manager, the tree's own keybinds on a
  selected node, `is_keybind`) still gives the request. So Super+W on an applet now asks it (`close_requested`) first.
  `spawn`/`quit` are node commands too (`n:spawn` runs in its tree's directory).

## Decisions

- Language: Luau (`mlua`, vendored, `async`, `send`), sandbox mode (read-only builtins, no `io`/`os.execute`).
  - Dynamic. Optional static typing later via a generated definitions file (`luau-lsp`/`luau-analyze`).
  - Modules are tables: `context.focus()`, `file_manager.from_node(n)`. Methods: `fm:open("/tmp")`.
- Types: `nil`, `boolean`, `number`, `string`, `{T}`, `T?`; handles are userdata named after the applet type
  (`FileManager`, `CommandHub`, `WlSurface`).
  - `Node` is a trait, not a type: every handle type registers the `Node` methods. No subtyping.
  - Method lookup uses the value's actual type: `n:open(p)` works if `n` is a file manager.
  - `n:cast(FileManager)` / `file_manager.from_node(n)`: `FileManager?`, nil on mismatch.
  - An applet command that clashes with a `Node` method is rejected at registration.
- Node identity: `NodeUid(u64)` from a global `AtomicU64`, globally unique across worlds (root + projects).
  - Registry (`sonamu_common`, `Mutex<HashMap>`): uid → (owning world's sender, local `NodeId`, type tag).
  - Worlds insert on add, remove on drop, re-point on moves between worlds.
  - Type tag: registered spawner name, or `wl_surface`; set at creation.
  - Not saved in sessions.
  - A handle to a removed node errors, never hits another node.
- Calls: `NodeCall { uid, name, args, reply }` (reply: oneshot) on the owning world's calloop channel; one handler in
  `NodeHolder` (root and projects alike).
  - Node commands: run on its `NodeTree`.
  - Applet commands: forwarded as `StandardEvent::Command { id, name, args }`; the applet answers with
    `StandardRequest::CommandResult { id, result }` (plumbing in `AppletClient`).
- Applets: the spawner declares commands statically (name, params with Luau types, return type, doc).
  `StandardApplet::command(&mut self, name, args: Vec<Value>) -> Result<Value, String>`
  (default: unknown command). Sync, on the applet's thread.
  `Value` is a plain serde enum, so applets don't depend on `mlua`.
- Script thread (`sonamu_script`, depends only on `sonamu_common`): one Luau VM, one persistent environment shared
  by hub and socket evals (like nvim's `:lua`). One eval at a time; the others queue.
  - Applet calls are `mlua` async functions waiting on `reply` (they yield, no deadlock: worlds never wait on the
    script thread).
  - `script::eval(code) -> (EvalHandle, Receiver<Output>)`; `EvalHandle::cancel()` sets a flag checked by the
    interrupt callback.
  - No timeouts in v0: Ctrl+C in the hub cancels its eval; a closed `sonamu-msg` connection cancels its eval.
  - Ctrl+C (hub or buffer line) also quits whatever runs, like Emacs' `C-g` (`sonamu_script::interrupt`: a static flag,
    cleared when the next job starts), so a stuck `init.luau`/binding/hook can be stopped. No deadlines.
  - `sonamu.reset()`: a fresh environment.
- Keybinds stay hardcoded but issue `NodeCall`s (Rust handlers, not Lua), so a stuck script can't block them.
- Hub: Luau only, calls `script::eval` directly.
  - Output: `print` as it comes, then return values formatted Lua-style (nothing for nil), errors in red.
  - Tab completion against the live environment: globals, fields of field chains (no calls), methods of a chain's
    actual handle type; after a call, the declared return type.
  - Old verbs: `add_child` → `n:add_child`, `run` → `process.spawn`, `$` → `hub:run_output`,
    `project` → `n:wrap_project()`, `help` → `sonamu.help()`; `test`, `addition` dropped.
- Socket: `Message::Eval(String)` with a reply (the socket is one-way today); `sonamu-msg eval '<code>'`:
  `print` to stderr, result as JSON on stdout, error → exit 1. `open` unchanged.

## v0 API

- `Node`: `close()`, `focus()`, `add_child(applet, args?): Node`,
  `swap_sibling(offset)`, `toggle_half()`, `wrap_project()`, `parent(): Node?`, `children(): {Node}`,
  `type(): string`, `title(): string`, `cast(T): T?`.
- `context.focus(): Node` (innermost, across worlds), `context.root(): Node`.
- `process.spawn(program, args?)`, `process.output(program, args?, dir?): string` (or with `fn(out, err)` last: doesn't wait, like nvim's `vim.system`;
  the program runs on its own thread, then `Call::Callback` queues `fn`; not cancellable); `clipboard.copy(text)`, `clipboard.paste(): string?`.
- `sonamu.open(applet, args?)`, `sonamu.reload_settings()`, `sonamu.quit()`, `sonamu.help(): string`, `sonamu.reset()`.
- `CommandHub`: `clear()`, `set_title(t)`, `run_output(program, args?): string`.
- `file_manager`: `root(): string`, `selected(): string?`, `open(path)` (shows its document).
- `code_editor`: `path(): string`, `text(): string`, `save()`.
- `wl_surface` (declared by the root, no spawner): `app_id(): string?`, `pid(): number?`.
- Not exposed: `Unwrap`, `Replace`, `StartWl`, `AddWlSurface` (sessions/plucking internals).

## Milestones (all done)

1. Identity: `NodeUid`, registry, type tag; worlds keep it updated.
2. Commands without Lua: `Value`, `NodeCall`, `NodeHolder` handler, applet `command()` + events; keybinds issue calls.
3. `sonamu_script`: thread, sandboxed env, bindings (incl. applet methods from spawner signatures, `cast`),
   `script::eval`, cancel.
4. Hub on Luau: input, `CommandHub` methods, output, completion.
5. Socket: `Message::Eval`, `sonamu-msg eval`, JSON output.

## To do

What to fix first and how: `.claude/COMMANDS_HANDOFF.md`.

Untested:
- Nothing here was tried in a running compositor (no headless sway on the dev machine): buffer line, palette
  colors, clipboard copy/paste with real clients, keybinds as commands, `parent()` across projects, `init.luau`.
- `sonamu.definitions()` not checked with luau-lsp.

Gaps:
- One queue: evals, completions, bindings and hooks run one at a time, so a slow `sonamu-msg eval` or sync
  `process.output` delays the buffer line and Luau keybinds (decided: keep it; Ctrl+C, and the callback form).
- Luau tables pass/return only their sequence part (`Value::List`); map keys are dropped silently.
- Prompt: no type-ahead while running; no newline key (only pasted ones).
- `hub:run_output` blocks the hub's thread (`process.output` doesn't).
- `cast` returns the same `Node`: no per-type handles, no `file_manager.from_node`.
- A node in JSON (socket) is its bare uid, not `{"type", "node"}`.
- A project node's `children()` are its outer children; no `Node` method reaches its inner tree (eg `n:inner()`).
- Buffer line: active screen only; output ≤ `BUFFER_OUTPUT_ROWS`, no scroll; shown only in buffer mode; history
  separate from each hub's.
- Events: only `focus` (no args) and `window` (app id); none for close, open, settings, screens; not over the socket.
- `keymap`: no unset; global only (no layering along the focus path, no `when` conditions).
- `init.luau` shares the one environment (no per-script environments, no `require`).
- Clipboard: applets can't copy from their own UI (only Luau's `clipboard.copy`); no primary selection.
- No effect levels / permissions on commands (`process.spawn`/`output` run anything), needed before agents.
- `docs/usage.md` doesn't mention Super+Esc or the buffer line (docs aren't edited from here).

Later:
- MCP bridge (tool schemas from the signatures), socket client libraries (Python, Go).
- Hub sugar: `context.focus` as a property narrowed by its live type (less needed: the buffer line sees the real
  focus).
- Indirect views (one applet in two nodes), node properties, subtree-scoped settings.
