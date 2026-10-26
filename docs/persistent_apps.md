# Persistent apps

Sonamu remembers your tree (layout, projects, native applets and Wayland apps) and rebuilds it when it starts.
Every Wayland window is saved with its command line, working directory, app id and title, and started again into
the same spot. While an app is starting, its spot shows "Starting …". Ctrl+W on a spot drops it.

Sonamu does the restoring, so turn off the apps' own session restore, or you'd get every window twice.

The session is saved every 2 seconds (if it changed) and before quitting, to `~/.local/state/sonamu/session.json`
(`$XDG_STATE_HOME/sonamu/session.json`). To use another file, set `SONAMU_SESSION=/path/to/file.json`.
To turn sessions off, set it to nothing (`SONAMU_SESSION=`). To start fresh, delete the file.
A file that can't be read is moved to `session.json.bad`.

The recommended apps are Firefox and foot: Super+T starts foot and Super+N starts Firefox.

## Firefox

Sonamu organizes the pages, so Firefox should open one page per window, and put each page's url in its title,
where Sonamu reads it from (before the first ` | `). Sonamu's extension does both. On restore, each window is
reopened with `firefox --new-window <url>`.

### The extension

Download
[sonamu.xpi](https://github.com/mathkimchi/singularity/releases/download/firefox-ext-v0.1.0/sonamu.xpi),
then in Firefox, open `about:addons`, click the gear, then Install Add-on From File. It's signed, so release Firefox
takes it. (To change it, see
[extras/firefox/README.md](../extras/firefox/README.md).)

To save private windows' urls too, allow it in private windows (`about:addons` → Sonamu → Run in Private Windows).

### Settings

| Setting (`about:config`) | Value | Why |
| --- | --- | --- |
| `browser.link.open_newwindow` | `2` | Links open new windows, not tabs |
| `browser.link.open_newwindow.restriction` | `0` | Even links that ask for a popup size |
| `browser.startup.page` | `0` | Sonamu restores the windows, so Firefox starts blank |
| `browser.sessionstore.resume_from_crash` | `false` | Not even after a crash. Quitting Sonamu ends Firefox without a clean shutdown, so without this every window comes back twice (turning off "Open previous windows and tabs" in Settings isn't enough) |
| `toolkit.legacyUserProfileCustomizations.stylesheets` | `true` | Load `userChrome.css`, which hides the tab bar |

The tab bar is hidden by `extras/firefox/userChrome.css`, copied to `chrome/userChrome.css` in your profile folder
(`about:support` → Profile Folder).

### NixOS with Home Manager

Install the extension by hand as above, then:

```nix
programs.firefox = {
  enable = true;
  profiles.default = {
    settings = {
      "browser.link.open_newwindow" = 2;
      "browser.link.open_newwindow.restriction" = 0;
      "browser.startup.page" = 0;
      "browser.sessionstore.resume_from_crash" = false;
      "toolkit.legacyUserProfileCustomizations.stylesheets" = true;
    };
    userChrome = ''
      #TabsToolbar { visibility: collapse !important; }
    '';
  };
};
```

### Without Home Manager

Install it (on NixOS, `programs.firefox.enable = true;`) and the extension, then add to `user.js` in your profile
folder:

```js
user_pref("browser.link.open_newwindow", 2);
user_pref("browser.link.open_newwindow.restriction", 0);
user_pref("browser.startup.page", 0);
user_pref("browser.sessionstore.resume_from_crash", false);
user_pref("toolkit.legacyUserProfileCustomizations.stylesheets", true);
```

and copy `userChrome.css` as above.

### Limits

All Firefox windows come from one process, so Sonamu matches restored windows to their spots by app id,
in the order they open. Two windows may come back in each other's spots.
Pages without a url in their title (eg a private window without the extension) come back blank.

## foot

Run plain `foot` for each window, not `foot --server` with `footclient`: with one process per window, Sonamu knows
which window is which and where its shell is. On restore, foot starts in the shell's last working directory.

The working directory is read when the window's title changes, so have the shell set the title on every prompt.
fish and nushell already do.

### NixOS with Home Manager

```nix
programs.foot = {
  enable = true;
  server.enable = false;
};

# bash: the working directory in the title on every prompt
programs.bash.initExtra = ''
  sonamu_title() { printf '\e]2;%s\a' "$PWD"; }
  PROMPT_COMMAND="sonamu_title''${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
'';

# or zsh
programs.zsh.initContent = ''
  autoload -Uz add-zsh-hook
  sonamu_title() { print -Pn '\e]2;%~\a' }
  add-zsh-hook precmd sonamu_title
'';

# or nushell (its default; set here in case your config turned it off)
programs.nushell.extraConfig = ''
  $env.config.shell_integration.osc2 = true
'';
```

On older Home Manager releases, `programs.zsh.initContent` is `programs.zsh.initExtra`.

### Without Home Manager

Install it (on NixOS, `environment.systemPackages = [ pkgs.foot ];`), don't enable the foot server
(`foot.service` / `foot-server`), and add the shell lines above to `~/.bashrc`, `~/.zshrc` or nushell's `config.nu`
(`$nu.config-path`). In `~/.bashrc`, write `${PROMPT_COMMAND:+;$PROMPT_COMMAND}` without the `''`.

### Keeping programs running across restarts

A restore starts a new shell, so whatever ran in it is gone. To keep it, run the shell in a session that outlives the
terminal: Sonamu saves the command line as is, so restoring reattaches.

```nix
home.packages = [ pkgs.shpool ];   # or pkgs.dtach / pkgs.abduco
```

Then open the terminal with a session name, eg `$ foot -e shpool attach notes`. Each window needs its own name.
The session survives Sonamu restarting, not a reboot.

## Nix notes

- Apps started from a `/nix/store/...` path are restored by name (eg `foot`), since the store path can be garbage
  collected after an update. Nix's `.foo-wrapped` binaries are restored through their wrapper `foo`.
  So keep the apps on your `PATH`, eg in `home.packages` or `environment.systemPackages`.
- Firefox and qutebrowser are restored by their app id, not their command line.

## Other apps

Any Wayland app is restored by its command line and working directory. Apps that open every window from one process
(eg `kitty -1`, Firefox) share one working directory, and are matched back by app id.
