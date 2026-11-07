# Sonamu Instruction Manual

## Running

In general, run with:

```sh
git clone https://github.com/mathkimchi/sonamu.git
cd sonamu
sh scripts/runner.sh # or just `cargo run --release --bin sonamu_sde`
```

On nix, run:

```sh
nix run github:mathkimchi/sonamu
```

Sonamu can be run directly from TTY or embedded inside another Wayland compositor.

## Basic Usage

Basic Usage Manual:
- Global Shortcuts:
  - `Super + Shift + /` (ie `Super + ?`): bring up this basic usage manual
  - `Super + T`: spawn terminal
  - `Super + N`: spawn browser
  - `Super + P`: wrap current tab in a project
  - `Super + W`: close current tab
  - `Super + F`: toggle half-view (like Niri)
  <!-- - `Super + Shift + \` (ie `Super + |`): split to right -->
  <!-- - `Super + Shift + -`: split to bottom -->
- Traversal Keys (use `Alt + <traversal key>` to focus the tree view):
  - (Shift) + Tab: goes (up)/down in the tree view
  - W/S: goes to prev/next sibling
  - A/D: goes to parent/child
- Actions while focused on tree view:
  - Traversal keys
  - Enter: focus on main content
  - Space: toggle children visibility
  - Click top right circle: pin/unpin tree view
  - Double click on node: makes it the view root
  - Drag/drop nodes: yes
  - Right click: brings up options
- Gestures:
  - 3-finger swipe up/down: next/previous sibling (the view follows the fingers)
  - 3-finger swipe left/right: child (the one last focused)/parent
  - 4-finger swipe up/down: focus tree view/main content

Niche keybinds not included,
and these can be configured as well.

## Configuration

Settings can be configured with
a simple json `~/.config/sonamu/settings.json`
or
an executable `~/.config/sonamu/settings`,
which should print a json.

The default configuration (that has every key shown) is in
`../assets/default_settings.json`
and my personal configuration can be found in
`../examples/config/settings`.

JSON schema is found in `../assets/settings.schema.json`.
The default configuration shows how to import it.
