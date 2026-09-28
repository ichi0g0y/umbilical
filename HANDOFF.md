# HANDOFF: Umbilical

Umbilical is a tray app. It keeps `claude remote-control` running in many project directories, so you can use them from claude.ai/code or the Claude mobile app.

The name comes from the umbilical cable (like in Evangelion): if the cable is cut, the app connects it again right away.

## Decisions

| Item | Decision |
|---|---|
| Name | **Umbilical** (binary / crate / bundle / log dir: `umbilical`, bundle ID e.g. `io.ichi0g0y.umbilical`) |
| Language / framework | **Rust + Tauri 2** |
| Platforms | **macOS and Windows** |
| Form | **GUI app**: tray icon + status window. Headless mode is **later** (keep the supervisor core separate from the GUI so it is easy to add). |
| Targets | Every directory directly under a root directory. Default root: **`~/.umbilical/repos`**. No default exclusions. New directories become targets automatically. Symlinks to real project directories are followed. **Targets are set from the GUI settings** (folder picker): root directories to scan, plus single directories added by hand. |
| Per-directory settings | **Yes.** Each directory can override the global settings (spawn mode, permission mode, extra args, enabled / disabled, session name, etc.). Edit from the GUI. |
| Spawn mode | Default **`worktree`** (new sessions from remote get their own git worktree). Configurable. If a directory is not a git repo, fall back to `same-dir`. |
| Session name | `--name <dir name>` (e.g. `nex`) |
| Config | `~/.umbilical/config.toml` (on Windows `~` is `%USERPROFILE%`): root, exclusions, spawn mode, permission mode, claude path |
| Permissions | `--permission-mode bypassPermissions` |
| Persistence | Starts automatically at user login |
| Repository | **Public** GitHub repo **`ichi0g0y/umbilical`** |
| Code signing | **None** (no Apple Developer ID, no notarization, no Windows cert). The Tauri updater signing key is still used (it is free and required by the updater). |
| Auto update | **Tauri updater plugin + GitHub Releases.** Two channels: **stable** and **nightly**. See "Auto update" below. |
| Settings | Almost everything is configurable (see "Config" below). |
| Terminal access | **Not needed.** Nobody needs to type into the remote-control TUI. The app shows status, errors and logs instead. |

### Why not tmux + launchd?

Claude Code has no built-in way to keep `remote-control` running (checked `claude remote-control --help` on 2.1.283). On macOS only, tmux + launchd would be enough. But we also want Windows, and tmux and launchd do not exist there. One Rust app works on both.

### Why Tauri 2 and not Wails (Go)?

- Tauri 2 has a stable tray API, an autostart plugin and a single-instance plugin.
- `portable-pty` (from WezTerm) works well on Unix pty and Windows ConPTY.
- Wails v2 has no tray. Wails v3 has one, but we did not check if it is stable.

## Status (2026-09-28, end of the first chat)

**Read this section first.**

### Working now (checked on this Mac)

- Umbilical is installed at `/Applications/Umbilical.app` and runs from there (autostart LaunchAgent points there). Root is `~/abyss` (set in the first-run screen). All folders there run `claude remote-control`, are connected, and show on claude.ai/code and in Claude Desktop.
- **Daemon**: sessions run in `Umbilical --daemon` (one process for all folders). The GUI talks to it over TCP 127.0.0.1 with a token from `~/.umbilical/daemon.json` (mode 600); `daemon.lock` stops a second daemon. Quitting the GUI keeps the sessions. The GUI replaces the daemon when its own binary is a different path or newer (dev rebuild, update); sessions restart once then. Tray: "Quit (sessions keep running)" / "Stop all sessions and quit". Checked: GUI quit -> daemon and 11 sessions stay; GUI reopen -> same daemon.
- `crates/umbilical-core`: config, folder scan, pty processes, backoff, state detection, auto trust (`~/.claude.json`), answers to prompts. 23 unit tests + 4 end-to-end tests with a fake `claude`.
- `src-tauri` + `ui`: tray, status window (icon buttons: start / restart / stop; red dot on error), settings (folder picker, per-folder overrides, auto trust), first-run screen (asks for the root), login dialog (`claude auth login` from the app), consent banner, autostart, single instance, updater (stable / nightly).
- `.github/workflows`: CI, stable release (tag `v*`), nightly. **Never run yet.**
- Nothing is committed. No GitHub repo yet.

### Things we learned about `claude remote-control` (2.1.283)

- On the first start it asks **`Enable Remote Control? (y/n)`** with no line break, and waits. Until you answer, it does not connect and the session does not show anywhere. Umbilical detects the text in the unfinished line (`LineSplitter::pending`) and shows a banner "Enable all" / per-folder "Enable". It sends `y` only when the user clicks. After one yes, `~/.claude.json` has `remoteDialogSeen: true`, so it may not ask again (check this after the next restart).
- If the parent process has `CLAUDECODE`, `CLAUDE_CODE_*` env vars (Umbilical started from inside a Claude Code session), the child says **"You must be logged in to use Remote Control"**. Umbilical now removes these vars for children (`child.rs`, `is_parent_claude_var`). An earlier idea that the macOS Keychain was the cause was wrong.
- Not trusted folder: **`Error: Workspace not trusted. Please run claude in <dir> first`**, then exit. Umbilical now sets `projects["<path>"].hasTrustDialogAccepted = true` in `~/.claude.json` for the roots and each target before start (`trust.rs`, setting `auto_trust`, default on).
- Normal start prints "Take this session with you ... Press Ctrl+C to stop." No session URL is printed, so `session_url` stays empty.
- `open`-ing the app from a shell passes that shell's env to the app.

### Logs (for debugging)

- `~/Library/Logs/Umbilical/umbilical.log`: daemon + GUI log (start, stop, restarts, trust, errors). Rotates at 10 MB.
- `~/Library/Logs/Umbilical/sessions/<folder>.log`: screen output of each session (TUI redraws removed).
- `~/Library/Logs/Umbilical/sessions/<folder>.claude-debug.log`: claude's own debug log (`--debug-file`), only when `debug = true`.
- `~/Library/Logs/Umbilical/login.log`: output of "Log in…".
- Setting `debug` (default off; **on now on this Mac for development**) adds `[debug]` lines: commands, started command lines, env keys (no values), last output before exit, attention changes, config reloads. Changing it restarts the sessions. claude's debug logs are not rotated by Umbilical yet.

### Notes

- Right after the daemon is replaced, each session may fail once with `Error: timeout of 15000ms exceeded`, then connect on the next try (5s later). This is fine.
- Output now has `https://claude.ai/code?environment=env_...`, so `session_url` is filled.
- `task dev` stops a running built copy first (single instance). The dev GUI then replaces the daemon with the debug binary. After dev, open `/Applications/Umbilical.app` again to switch back.
- A tmux backend was discussed and dropped: tmux must be installed, and the daemon also works on Windows.

### Next steps (ask the user before 1-3)

1. Commit (no Claude attribution lines in commits or PRs; the user said so).
2. Create the public repo `ichi0g0y/umbilical`, push, set secrets `TAURI_SIGNING_PRIVATE_KEY` (from `~/.tauri/umbilical.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty). Then check CI, the Windows build and the nightly version format with NSIS.
3. Done: installed to `/Applications`.
4. Small fixes:
   - The consent answer was sent twice when "Enable all" was clicked twice. The UI now disables the buttons after a click. The supervisor could also ignore the same prompt for a few seconds after an answer.
   - Old per-folder logs in `~/Library/Logs/Umbilical/*.log` (before the move to `sessions/`) can be deleted. `umbilical.log` has some old session lines mixed in.
   - Better icon (placeholder now). LICENSE holder is `ichi0g0y` (robco uses `nantokaworks`; ask).
   - Windows: check the `~/.claude.json` key format for paths (we guess forward slashes), and test on a real machine.

## Design

### Supervisor core (no GUI code here)

- Scan `<root>/*/` every few seconds. Skip hidden directories. Read exclusions from the config file. Follow symlinks.
- For each directory, spawn `claude remote-control --permission-mode bypassPermissions --spawn <mode> --name <dir name>` **inside a pty** (`portable-pty`), with `cwd` set to that directory.
- Always read the pty output and write it to a log file. If nothing reads it, the buffer fills and the child stops.
  - macOS: `~/Library/Logs/Umbilical/sessions/<dir>.log` (app log: `~/Library/Logs/Umbilical/umbilical.log`)
  - Windows: `%LOCALAPPDATA%\Umbilical\logs\sessions\<dir>.log`
- Detect state from the output and the exit code:
  - running / waiting to restart (seconds left) / stopped at a prompt (trust dialog, bypass warning) / login expired / stopped
  - Keep the last error line and the start time.
- When a child exits: restart with exponential backoff (5s, up to 300s). If it ran for 10+ minutes, reset the backoff.
- Directory added: start it. Directory removed: stop it.
- On quit or on a signal, kill every child before exit.
- The GUI talks to the core with a command channel (Restart(name) / Stop(name) / RestartAll / Shutdown) and reads a status snapshot.

### Platform notes

- **macOS PATH**: apps started at login get a very small PATH. At startup, get PATH from `/bin/zsh -lic 'print -r -- $PATH'` (last line) and pass it to the children.
- **Windows PATH**: GUI apps get the user PATH, so this is mostly not needed.
- claude path: `CLAUDE_BIN`, or `~/.local/bin/claude` (macOS). Find the right default on Windows.
- **Windows: do NOT use a Windows service.** A service runs in session 0 as another user, so it cannot see the user's Claude login. Start at user logon as the user.
- **Unchecked**: does `claude remote-control` work on native Windows under ConPTY? Test this by hand first.

### GUI

- Tray / menu bar:
  - Title or tooltip like `RC 7/9` (running / total). On macOS, use `ActivationPolicy::Accessory` so it has no Dock icon.
  - Menu: Open window, Restart all, Open logs folder, Quit.
- Status window (open from the tray):
  - One row per directory: state, last error, uptime.
  - Live log tail for the selected directory.
  - Restart / Stop buttons.
- Settings: edit the config from the window, or open the config file. Reload it without restart.
- Autostart: Tauri autostart plugin (LaunchAgent / login item on macOS, Run key on Windows).
- Single instance: Tauri single-instance plugin.

### Auto update

- Use `tauri-plugin-updater`. The app checks a `latest.json` file on GitHub Releases, downloads the new bundle, checks the signature and installs it.
- Updates must be signed with a Tauri updater key (`tauri signer generate`). Keep the private key in GitHub Actions secrets.
- **stable channel**: built when a `v*` tag is pushed. Published as a normal release. Endpoint: `https://github.com/ichi0g0y/umbilical/releases/latest/download/latest.json`.
- **nightly channel**: a scheduled GitHub Actions workflow (and manual run) builds the latest `main` every night. It is published to one pre-release with a fixed tag `nightly` (delete and recreate the assets each time). Endpoint: `https://github.com/ichi0g0y/umbilical/releases/download/nightly/latest.json`.
  - Nightly version: `<next patch>-nightly.<YYYYMMDDHHMM>` so that newer nightlies are always "greater".
  - Switching from nightly back to stable: allow a "downgrade" to the latest stable when the user changes the channel.
- CI builds macOS (universal or arm64 + x64) and Windows (x64) with `tauri-action`.
- Before installing an update, the app stops all children cleanly, then restarts itself. After restart, the supervisor starts the children again.

### Config

`~/.umbilical/config.toml`. Editable from the settings screen. Reload without restart when possible.

- `roots` (list, default `["~/.umbilical/repos"]`), `dirs` (single directories added by hand), `exclude` (list)
- `scan_interval_secs`
- `claude_bin`, `permission_mode` (default `bypassPermissions`), `spawn` (default `worktree`), `capacity`, extra args
- `backoff_min_secs` (5), `backoff_max_secs` (300), `stable_reset_secs` (600)
- `log_dir`, log size limit / rotation
- `autostart` (on / off)
- `update.channel` (`stable` / `nightly`), `update.check_on_start`, `update.check_interval_hours`, `update.auto_install` (install silently, or only notify)
- `[overrides."<path>"]` per-directory overrides: `enabled`, `name`, `spawn`, `permission_mode`, `capacity`, `extra_args`, env vars

The GUI settings screen has: a list of roots and single directories (add with a folder picker, remove), a detail panel per directory for overrides, and global settings. The config file stays the source of truth, so it can also be edited by hand.

### No code signing: what it means

- **macOS**: the first launch is blocked by Gatekeeper. The user opens it once with right-click > Open (or `xattr -dr com.apple.quarantine Umbilical.app`). Apple Silicon needs at least an ad-hoc signature; the build does this (`signingIdentity: "-"`). Updates downloaded by the app itself do not get the quarantine flag, so auto update should work without new warnings (check this).
- **Windows**: SmartScreen shows a warning on the first install ("More info" > "Run anyway"). Check how the updater installer behaves.
- Write these steps in the README.

## Prerequisites (confirm with the user)

1. **Trust**: handled by Umbilical (`auto_trust`). Without it, run `claude` once in each folder.
2. **Bypass warning**: if the bypass warning screen blocks things, set `"skipDangerousModePermissionPrompt": true` in `~/.claude/settings.json`.
3. **After a reboot**: the app starts only after login. **Decided: turn on macOS automatic login** (the user does this in System Settings).
4. **Security**: anyone who can use the Claude account can run any command in the target directories with no confirmation. **Always turn on two-factor authentication.**

## Notes from earlier chats

- In auto mode, the Claude Code safety classifier denied reading LaunchAgent config and `cargo new`. Do not work around denials. Build, run and persistence setup happen only after the user approves.

## Done when

- After start, every target directory shows as a session in claude.ai/code (on macOS and Windows).
- Killing one child brings it back after the backoff.
- Adding a new directory adds a session automatically.
- It comes back after a reboot and login.
- The app finds and installs a new stable release, and a new nightly build when the channel is `nightly`.
- The tray shows running / total. The window shows state, errors and logs. Restart and Quit work.
