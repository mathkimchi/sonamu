-- Runs once when Sonamu starts, and again on sonamu.reset() (see `help()` in the buffer line, Super+Esc).
-- What it defines stays in the buffer line's environment.

-- A global key: Super+Shift+F opens a file manager next to the focus
-- keymap.set({ "logo", "shift" }, "f", function()
-- 	sonamu.open("file_manager", { "/tmp" })
-- end)

-- Hooks
-- events.on("window", function(app_id)
-- 	print("opened", app_id)
-- end)
-- events.on("focus", function(node)
-- 	print("focus:", node:title())
-- end)
-- Also `open` (a new node), `close` (its node, gone), `settings`, `screens` (count);
-- outside Sonamu: `sonamu-msg subscribe focus close`
