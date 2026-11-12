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
- Hub: Luau only, one eval at a time (like a shell: no prompt until it ends; keys typed meanwhile edit it, Enter queues), `print`s then results (strings unquoted), Ctrl+C cancels; commands `clear`, `set_title`, `open` (replace the hub, as
  before; `"wl", {argv}` too). `run_output` (old `$`) dropped for `process.output`.
- Signatures: `AppletSpawnerTrait::commands` (`&'static [command::Signature]`: name, params, returns, doc), declared
  into `command::declare` when spawners register (`RootNodeApplet::new`, `register_applet_spawner`); clashes with
  `command::NODE_COMMANDS` dropped with a warning. Hub: `COMMANDS`. `sonamu.help()` (also global `help()`; a bare `help` in the hub runs it) lists them per type, and the hub keys.
- Tab: `sonamu_script::complete` (a job on the script thread, answered through the waker, dropped if the prompt
  changed): globals (incl. builtins), table fields after `.`, after `:` on a node its `Node` methods + its type's
  signatures. To know the node, completion runs read-only getters only: `context.focus()`, `context.root()`,
  `:parent()`, `:inner()` (deviation: Q22 said no calls); `:add_child()` gives a node of unknown type (`Node` methods only).
- Paste: Ctrl+V / Ctrl+Shift+V (`sonamu_common::clipboard::paste`) inserts at the cursor; newlines kept (shown `↵`).
  Ctrl+Shift+C copies the last entry's prints and result.

- Buffer line (root, `RootNodeApplet::{buffer_mode, repl}`): Super+Esc toggles buffer mode (like vim's normal
  mode); `sonamu_common::BUFFER_MODE` makes `is_keybind` send every key to the root then, and the runner
  (`runner::run_binding`) skip settings/`keymap.set` bindings on keys with a symbol unless Super is held (minibuffer-like
  shadowing; media/F-keys and Super combos still work). Esc clears the prompt, or leaves if empty.
  Bottom of the active screen only (decided: like Emacs frames, the others show `-- INSERT --` or nothing): the last
  entry's output and Tab's candidates (≤ `BUFFER_OUTPUT_ROWS`) over the prompt; PageUp/PageDown scroll the whole
  history there (`buffer_scroll`, back to 0 on Enter).
  Setting `buffer_line` (`.claude/SETTINGS.md`): `always` reserves the row and shows `-- INSERT --` outside buffer mode.
  `context.focus()` there is the real focus (no "previous focus" problem). Not a node: no applet commands of its own.
- Up/Down recall (`repl::RECALL`) is one list for every prompt (nvim-like); outputs stay per prompt.
- `command_hub/repl.rs`: `Repl` (prompt, history, recall, evals, completion, paste, `lines`, `draw_prompt`; Shift+Enter
  a newline), shared by
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
process.output("ls", {"-a"})
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
- `cast` takes the type name (`n:cast("file_manager")`), returns the same `Node`; no per-type userdata (decided: one
  `Node` already dispatches by actual type). Typed instead by `<type>.from_node(n)`: a global table per declared type
  (`add_type_tables`, before each job, unless the name is taken), declared in `definitions()` as `-> <type>?`.
- No `split` (splits no longer exist). `parent()` of a project's inner root is the project node's parent (the two
  are one row): mains get `StandardEvent::NodeUid` (`NodeTree::register` and wherever a main is replaced; kept in
  `AppletContext::uid`), and `ProjectApplet::handle_node_call` asks its own node's world from a thread. The root
  tree's root has no parent (nil).

- Effects (`command::Effect`: read < modify < destructive < spawn): `Signature::effect`, `node_effect`,
  `tree_effect`, `applet_effect` (undeclared: destructive; `__` internal: read). `NodeCall::max` (`call_limited`) is
  checked by the node's world (`NodeHolder::handle_node_call`); `eval_limited` sets the eval's max, which its
  `call`s pass and its builtins check (`allow`: `process.output` spawn, `clipboard.copy` modify, `sonamu.reset`
  destructive, `keymap.set`/`events.on` spawn since they run later unlimited). Buffer line, hubs, init, bindings and
  hooks: unlimited. Socket: `sonamu-msg eval --max <effect>` (default unlimited). A limited eval runs in its own env
  (`require::own_environment`, reading through), so it can't redefine what unlimited bindings/hooks call; functions
  defined earlier that it calls are checked against its max.
- Socket: `Message::Eval { code, max }`; the runner answers from a thread (`msg_socket::answer_eval`) with JSON lines
  (`msg::EvalReply`: `print`, then `done` `Ok(values)`/`Err`), plus empty keepalive lines every 100 ms, which fail once
  the sender is gone and cancel the eval (reading can't tell: it already shut its writing half). `sonamu-msg eval
  '<code>'` (`-`: stdin): prints to stderr, one value as JSON (several as an array) on stdout, error → exit 1. A node
  is `{"type": .., "node": uid}` (`msg_socket::to_json` asks its type when answering; null if gone).

- Startup / keys / hooks (`sonamu_script`): `start()` (the root, at init) runs `<config dir>/init.luau` (again after
  `sonamu.reset()`, which also drops bindings), in the shared env, as chunk `@<path>`. `require("./name")`
  (`require.rs`, `ConfigRequirer`): relative to the requiring file, or the config dir from evals (`=input`);
  `name.luau`/`.lua`/`name/init.luau`; no `.luaurc` aliases; cached per state. Each module has its own globals
  (reading through to the shared env; Roblox model) and shares what it returns (decided over a shared env). `keymap.set({mods}, key, fn, {type = t}?)`: keys in the static `BINDINGS`
  (matched like settings `Keybind`s; same key and type replaces); with `type`, only while the innermost focus is of
  that applet type, and it wins over a global one (decided: like Emacs major-mode maps; over focus-path layering or
  `when` predicates, which would run Luau per key). `FOCUS_TYPE` is set when the script thread resolves a `focus`
  event (so it lags a moment, and misses a main replaced in place). `keymap.unset({mods}, key, {type}?)`: whether it
  was bound, fns in the state's registry; the runner checks
  `binding(..)` after the settings keybinds (before the root, so any focus) and queues `run_binding`.
  `events.on(event, fn)`, positional args: `focus(node)` (root emits it without args when `remember_focus` sees the
  path change; the script thread fills in `command::focus`, since worlds don't wait), `window(app_id)`
  (`WlSurfaceRegistered`), `open(node)` (`NodeTree::insert_node`, any world; not restores/moves), `close(node)`
  (`NodeTree::close`; the handle is gone, compare with `==`), `settings()`, `screens(count)` (count changed).
  `sonamu_script::subscribe` gets every emitted event: `sonamu-msg subscribe [event...]` (`Message::Subscribe`,
  `msg_socket::answer_subscribe`) prints them as `msg::EventLine` JSON lines. These run as `Job::Call`: no one waits; their `print`s and errors are messages (below).
- Messages (Emacs' `*Messages*` + echo area): `sonamu_script::message` keeps the last 200 (`sonamu.messages()`), logs
  them to stderr, and calls `on_message` (the root's waker); the root (`echo_message`) shows the last one over the
  active screen's buffer line row for 5 s (errors in red; `Bar::None` when the row isn't reserved). Decided over: errors
  as desktop notifications (prints only in the list), or the last shown only when buffer mode next opens.
  Example: `examples/config/init.luau` (all commented).

- `sonamu.definitions()`: luau-lsp definitions (`declare class Node`, a class per declared applet type named like it,
  extending `Node`; the globals), eg `sonamu-msg eval 'sonamu.definitions()' > sonamu.d.luau`. Not checked against
  luau-lsp yet. `cast` returns `Node`; `file_manager.from_node(n)` is typed `file_manager?`.

- Keybinds: `standard_keybinds::standard_keybind` maps a key to a `KeybindCommand` (`quit`, `wrap_project`, `close`,
  `toggle_half`, `spawn(program)`, `spawn_sibling(program)` (Super+Shift+T/N: `NodeRequest::RunSibling`, window goes
  right after the node via `wl_target`; a project's inner root passes it up), `swap_sibling(±1)`); `handle_standard_keybinds` sends it to the applet's own node
  (`command::send`, no answer; as the old `NodeRequest` via `KeybindCommand::request` until `AppletContext::uid` is
  known). `standard_keybind_request` (wl tiles, session placeholders, file manager, the tree's own keybinds on a
  selected node, `is_keybind`) still gives the request. So Super+W on an applet now asks it (`close_requested`) first.
  `spawn`/`quit` are node commands too (`n:spawn` runs in its tree's directory).

## Decisions

- Language: Luau (`mlua`, vendored, `async`, `send`), sandbox mode (read-only builtins, no `io`/`os.execute`).
  - Dynamic. Optional static typing later via a generated definitions file (`luau-lsp`/`luau-analyze`).
  - Modules are tables: `context.focus()`, `file_manager.from_node(n)`. Methods: `fm:open("/tmp")`.
- Types: `nil`, `boolean`, `number`, `string`, `{T}`, `T?`, tables with string keys (`Value::Map`: a table is a
  `List` if all its keys are 1..n, else string keys only; JSON object; `{k = v}`); handles are userdata named after the applet type
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
  - Old verbs: `add_child` → `n:add_child`, `run` → `process.spawn`, `$` → `process.output` (`hub:run_output` was dropped),
    `project` → `n:wrap_project()`, `help` → `sonamu.help()`; `test`, `addition` dropped.
- Socket: `Message::Eval(String)` with a reply (the socket is one-way today); `sonamu-msg eval '<code>'`:
  `print` to stderr, result as JSON on stdout, error → exit 1. `open` unchanged.

## v0 API

- `Node`: `close()`, `focus()`, `add_child(applet, args?): Node`,
  `swap_sibling(offset)`, `toggle_half()`, `wrap_project()`, `parent(): Node?`, `children(): {Node}`,
  `type(): string`, `title(): string`, `cast(T): T?`, `inner(): Node?` (the root of the world in its applet: a
  project's tree; `command::INNER_ROOT`, nil for other applets; `children()` stay outer).
- `context.focus(): Node` (innermost, across worlds), `context.root(): Node`.
- `process.spawn(program, args?)`, `process.output(program, args?, dir?): string` (or with `fn(out, err)` last: doesn't wait, like nvim's `vim.system`;
  the program runs on its own thread, then `Call::Callback` queues `fn`; not cancellable); `clipboard.copy(text, {primary}?)`, `clipboard.paste({primary}?): string?`.
- `sonamu.open(applet, args?)`, `sonamu.reload_settings()`, `sonamu.quit()`, `sonamu.help(): string`, `sonamu.reset()`.
- `CommandHub`: `clear()`, `set_title(t)`, `open(applet, args?)`.
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


Untested:
- Tried live (headless sway, 2026-11-10, over `sonamu-msg`): `init.luau`, `focus(node)` events, node JSON, effect
  limits, signature errors, `from_node`, `inner()`, `subscribe`, tables. Not yet (injected keys didn't arrive): the
  buffer line UI and echo area, palette colors, keybinds as commands, `keymap.set` keys, clipboard with real
  clients, `parent()` across projects.
- `sonamu.definitions()` not checked with luau-lsp.

Gaps:
- One queue: evals, completions, bindings and hooks run one at a time, so a slow `sonamu-msg eval` or sync
  `process.output` delays the buffer line and Luau keybinds (decided: keep it; Ctrl+C, and the callback form).
- Clipboard: the code editor can't copy (it has no selection yet; add a copy key once it has one).
- `n:type()` right after `add_child` is `applet`: the derived type waits for the applet's first treeview publish
  (`saved`); `cast`/`from_node`/typed bindings miss it until then.
- `docs/usage.md` doesn't mention Super+Esc or the buffer line (docs aren't edited from here).

Later:
- MCP bridge (tool schemas from the signatures), socket client libraries (Python, Go).
- Hub sugar: `context.focus` as a property narrowed by its live type (less needed: the buffer line sees the real
  focus).
- Indirect views (one applet in two nodes), node properties, subtree-scoped settings.
