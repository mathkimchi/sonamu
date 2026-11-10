# Commands and scripting (in progress)

Emacs-like model: every action is a named command; keybinds, the hub, the socket and (later) agents all call the same ones.
Embedded language: Luau via `mlua`. Other languages: over the socket.

## Status (milestones 1, 3 and most of 4 done)

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

Try in a hub:

```lua
sonamu.help()
context.focus():title()                     -- "Command Hub": in the hub, focus is the hub
for _, n in context.root():children() do print(n:type(), n:title()) end
context.focus():parent():add_child("file_manager", {"/tmp"})
context.focus():set_title("Lua hub")        -- applet command, via __index
context.focus():run_output("ls", {"-a"})
x = 5                                       -- then `x * 2` gives 10
while true do end                           -- Ctrl+C cancels
```

Deviations from the decisions below:
- Calls block the script thread instead of `mlua` async (same effect: worlds never wait on it).
- Type tag: derived when asked (`NodeTree::applet_type`: wl surface, else saved/hibernated spawner name, else
  `container`/`applet`), not stored.
- `cast` takes the type name (`n:cast("file_manager")`), returns the same `Node`; no per-type handles.
- No `split` (splits no longer exist). `parent()` stops at its world's root (nil there, even inside a project).
- `hub:run_output` blocks the hub's thread while the program runs.

Not done: keybinds through `NodeCall`, socket `eval`; signatures aren't checked against calls (`command()` is).

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
- `process.spawn(program, args?)`.
- `sonamu.open(applet, args?)`, `sonamu.reload_settings()`, `sonamu.quit()`, `sonamu.help(): string`, `sonamu.reset()`.
- `CommandHub`: `clear()`, `set_title(t)`, `run_output(program, args?): string`.
- Not exposed: `Unwrap`, `Replace`, `StartWl`, `AddWlSurface` (sessions/plucking internals).

## Milestones

1. Identity: `NodeUid`, registry, type tag; worlds keep it updated.
2. Commands without Lua: `Value`, `NodeCall`, `NodeHolder` handler, applet `command()` + events; keybinds issue calls.
3. `sonamu_script`: thread, sandboxed env, bindings (incl. applet methods from spawner signatures, `cast`),
   `script::eval`, cancel.
4. Hub on Luau: input, `CommandHub` methods, output, completion.
5. Socket: `Message::Eval`, `sonamu-msg eval`, JSON output.

## Later

- User-rebindable keymaps (layered along the focus path; VS Code-style `when` conditions).
- Hooks/events (over the socket too).
- Script files, each with its own environment, sharing through `require` (Roblox model).
- Generated Luau definitions file → optional static typing, editor completion.
- Hub sugar: `context.focus` as a property narrowed by its live type; a "previous focus" (in the hub, focus is the
  hub itself).
- MCP bridge (tool schemas from the signatures), socket client libraries (Python, Go).
- Indirect views (one applet in two nodes), node properties, subtree-scoped settings.
