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

-- the status bar's segments (#150), left and right, each left to right; this is the default
-- sonamu.bar_layout({"tree", "launcher", "time"}, {"tray", "bell", "resources", "hub"})

-- Beeper starts in the tray: settings' `startup` leaves a flag and starts it; its first window is closed, which hides
-- it to the tray (unless Beeper's quit_on_close is set). Only once: the flag is gone after, even on sonamu.reset().
local function find_window(node, name)
	local window = wl_surface.from_node(node)
	if window and (window:app_id() or ""):lower():find(name, 1, true) then
		return node
	end
	local inner = node:inner()
	local found = inner and find_window(inner, name)
	if found then
		return found
	end
	for _, child in node:children() do
		found = find_window(child, name)
		if found then
			return found
		end
	end
	return nil
end
events.on("window", function(app_id)
	if not app_id or not app_id:lower():find("beeper", 1, true) then
		return
	end
	local flag = process.output("sh", { "-c", 'rm "$XDG_RUNTIME_DIR/sonamu-hide-beeper" 2>/dev/null && echo yes; true' })
	if flag ~= "yes\n" then
		return
	end
	local window = find_window(context.root(), "beeper")
	if window then
		window:close()
	end
end)
