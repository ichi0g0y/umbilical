# Umbilical

Umbilical is a small tray app for macOS and Windows (Linux builds too, not tested much). It keeps `claude remote-control` running in your project folders, so you can always open them from [claude.ai/code](https://claude.ai/code) or the Claude mobile app.

- It watches every folder directly inside a root folder (default `~/.umbilical/repos`). Symlinks work, so you can link your real projects there.
- It starts one `claude remote-control` per folder, inside a pseudo terminal.
- If a process stops, it starts it again. The wait gets longer each time (5s, 10s, … up to 300s). After 10 minutes of good running, the wait goes back to 5s.
- New folders are added and removed folders are stopped automatically.
- The tray shows `running/total`. The window shows the state, errors and output of each folder, and how many sessions are working now.
- You choose where it shows (menu bar, Dock, or both) and if the window opens at start.
- It starts at login and updates itself from GitHub Releases (stable or nightly). **Updates wait until no session is working**, because the restart stops running turns ("Install now" does not wait). "Check for Updates…" is in the tray and app menus.
- The sessions run in a small background daemon (`Umbilical --daemon`). **Quitting the window/tray app does not stop them.** Use "Stop all sessions and quit" in the tray menu to stop everything.
- It marks the target folders as trusted in Claude Code, and shows a banner when claude asks "Enable Remote Control? (y/n)" (answer once with **Enable all**).
- On macOS, the **Permissions** tab shows and asks for the permissions your sessions need (Accessibility, Screen Recording, Automation, …).
- After a restart, `claude remote-control` reconnects the session it made in each folder, and the chat goes on. A turn that was running is stopped. Sessions made from the web in their own worktree are not started again; their files stay.

## Install

Download the latest build from [Releases](https://github.com/ichi0g0y/umbilical/releases):

- macOS: `Umbilical_<version>_aarch64.dmg` (Apple Silicon) or `_x64.dmg` (Intel)
- Windows: `Umbilical_<version>_x64-setup.exe`
- Linux: `Umbilical_<version>_amd64.AppImage` or `.deb`

The builds are **not signed by Apple or Microsoft**, so the OS warns you the first time:

- **macOS**: Gatekeeper blocks the first launch. Right-click the app and choose **Open**, or run
  `xattr -dr com.apple.quarantine /Applications/Umbilical.app`.
- **Windows**: SmartScreen shows "Windows protected your PC". Click **More info** and then **Run anyway**.

Later updates are downloaded by the app itself, so they should not show these warnings again.

### macOS permissions

Sessions run inside Umbilical, so macOS uses Umbilical's permissions for them (Accessibility, Screen Recording, and so on). Open the **Permissions** tab to see them and allow what your sessions need. Then click **Restart all**.

The macOS builds are signed with our own (self-signed) certificate. It is the same for every version, so macOS keeps the permissions after an update. (Builds before 0.1.3 used another signature or app ID. After you update from them, allow the permissions once more.)

## Before the first start

1. **Log in**: `claude` must be logged in with an account that has a subscription.
2. **Trust**: Umbilical marks every target folder as trusted in `~/.claude.json` (setting "Trust every target folder", on by default).
3. **Bypass warning**: the default permission mode is `bypassPermissions`. If the warning screen blocks the sessions, add this to `~/.claude/settings.json`:
   ```json
   { "skipDangerousModePermissionPrompt": true }
   ```
4. **Security**: with `bypassPermissions`, anyone who can use your Claude account can run any command in these folders without asking. **Turn on two-factor authentication.**

If a folder is stuck on one of these screens, the window shows it with a red mark and tells you what to do.

## Settings

Open the window from the tray and go to **Settings**. Everything is saved to `~/.umbilical/config.toml` (Windows: `%USERPROFILE%\.umbilical\config.toml`). You can also edit the file by hand; the app reloads it.

```toml
roots = ["~/.umbilical/repos"]   # every folder inside is a target
dirs = []                         # single folders
exclude = []                      # folder names or full paths
scan_interval_secs = 5

claude_bin = ""                   # empty: ~/.local/bin/claude or PATH
permission_mode = "bypassPermissions"
spawn = "worktree"                # same-dir | worktree | session (worktree needs git)
extra_args = []

backoff_min_secs = 5
backoff_max_secs = 300
stable_reset_secs = 600

log_dir = ""                      # empty: ~/Library/Logs/Umbilical or %LOCALAPPDATA%\Umbilical\logs
log_max_bytes = 5242880
show_in = "menu_bar"              # menu_bar, menu_bar_and_dock or dock (Windows: taskbar)
start_window = "manual"           # open the window at start: manual (not at login), always or never
autostart = true

[env]
# KEY = "value"

[update]
channel = "stable"                # stable | nightly
check_on_start = true
check_interval_hours = 6
auto_install = false

# Per-folder settings. Anything you leave out uses the values above.
[overrides."/Users/me/.umbilical/repos/nex"]
spawn = "same-dir"
name = "nex-main"
```

## Development

Tools: Rust (stable), [Bun](https://bun.sh), and optionally [Task](https://taskfile.dev).

```sh
bun install
task dev      # run the app (uses your real config!)
task check    # UI build, fmt, clippy, tests
task build    # build a local .app / installer
```

Layout:

- `crates/umbilical-core`: the supervisor (config, folder scan, pty processes, restarts, trust) and the daemon (`daemon.rs`: TCP on 127.0.0.1 + token in `~/.umbilical/daemon.json`). No GUI code.
- `src-tauri`: the Tauri 2 app (tray, window, commands, updater). `umbilical --daemon` runs only the daemon; the GUI starts it when needed and replaces it when the binary is newer.
- `ui`: the window UI. React + TypeScript + [shadcn/ui](https://ui.shadcn.com) + Tailwind CSS, state in [Jotai](https://jotai.org), built with Vite. `bun run --cwd ui dev` serves it for `tauri dev`; `bun run ui:build` writes `ui/dist`, which the app embeds.

## Releases

- **Stable**: `task release -- 0.1.0` pushes the tag `v0.1.0`. GitHub Actions builds macOS (arm64, x64), Windows (x64) and Linux (x64) and publishes the release. The tag sets the app version.
- **Nightly**: every night (03:00 JST) GitHub Actions builds `main` if it changed, and replaces the `nightly` pre-release. The version is `<next patch>-nightly.<YYYYMMDDHHMM>`.

The updater needs these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`: the private key from `bunx tauri signer generate` (the public key is in `src-tauri/tauri.conf.json`).
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password (can be empty).

## License

MIT

## Support

Umbilical is free and stays free. If it helps you, [buy me a coffee](https://buymeacoffee.com/ichi0g0y) or [sponsor on GitHub](https://github.com/sponsors/ichi0g0y).
