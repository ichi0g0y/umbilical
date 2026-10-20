// Typed calls to the Rust side (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export type RunState = "running" | "waiting" | "stopped" | "disabled";
export type Attention = "trust_prompt" | "bypass_prompt" | "login_required" | "remote_control_consent";

export interface DirStatus {
  key: string;
  name: string;
  path: string;
  source: string;
  state: RunState;
  pid: number | null;
  started_at: number | null;
  next_restart_at: number | null;
  restarts: number;
  last_exit: string | null;
  last_error: string | null;
  attention: Attention | null;
  /** Waiting because an old registration still holds the folder. Not an error. */
  already_served: boolean;
  session_url: string | null;
  command: string | null;
  spawn: string;
  effective_spawn: string;
  log_path: string;
  /** Sessions working right now. */
  busy: number;
  /** Remote Control sessions kept to resume after a restart. */
  kept_sessions: number;
  /** Remote Control sessions started again in their worktree. */
  resumed: ResumedSession[];
  /** The last session that could not be resumed. */
  resume_note: string | null;
}

export interface ResumedSession {
  id: string;
  state: RunState;
  pid: number | null;
  started_at: number | null;
  next_restart_at: number | null;
  /** Failed starts in a row. */
  tries: number;
  last_exit: string | null;
  last_error: string | null;
  session_url: string | null;
  log_path: string;
}

export interface Snapshot {
  dirs: DirStatus[];
  running: number;
  total: number;
  config_path: string;
  config_error: string | null;
  /** The config had an error at start, so nothing runs until it is fixed. */
  config_held: boolean;
  log_dir: string;
  claude_bin: string | null;
  needs_setup: boolean;
  daemon_error: string | null;
  /** When the config file was last changed (unix ms). */
  config_mtime: number | null;
}

export interface DirOverride {
  enabled?: boolean | null;
  name?: string | null;
  claude_bin?: string | null;
  permission_mode?: string | null;
  spawn?: string | null;
  capacity?: number | null;
  skip_first_session?: boolean | null;
  resume_sessions?: boolean | null;
  extra_args?: string[] | null;
  env?: Record<string, string>;
}

export interface UpdateConfig {
  channel: "stable" | "nightly";
  check_on_start: boolean;
  check_interval_hours: number;
  auto_install: boolean;
}

/** "system" follows the OS. Every other theme is light or dark by itself. */
export type ThemeName =
  | "system"
  | "umbilical-light"
  | "umbilical-dark"
  | "catppuccin-latte"
  | "catppuccin-frappe"
  | "catppuccin-macchiato"
  | "catppuccin-mocha"
  | "dracula-light"
  | "dracula-dark"
  | "nord-light"
  | "nord-dark"
  | "solarized-light"
  | "solarized-dark"
  | "gruvbox-light"
  | "gruvbox-dark"
  | "tokyo-night-light"
  | "tokyo-night-dark"
  | "rose-pine-light"
  | "rose-pine-dark"
  | "one-light"
  | "one-dark"
  | "github-light"
  | "github-dark";

export interface AppearanceConfig {
  theme: ThemeName;
}

export type ShowIn = "menu_bar" | "menu_bar_and_dock" | "dock";
export type StartWindow = "manual" | "always" | "never";

export interface Config {
  roots: string[];
  dirs: string[];
  exclude: string[];
  scan_interval_secs: number;
  claude_bin: string;
  permission_mode: string;
  spawn: string;
  capacity: number | null;
  skip_first_session: boolean;
  extra_args: string[];
  env: Record<string, string>;
  resume_sessions: boolean;
  auto_trust: boolean;
  claude_json: string;
  backoff_min_secs: number;
  backoff_max_secs: number;
  stable_reset_secs: number;
  log_dir: string;
  log_max_bytes: number;
  debug: boolean;
  autostart: boolean;
  show_in: ShowIn;
  tray_count: boolean;
  start_window: StartWindow;
  update: UpdateConfig;
  appearance: AppearanceConfig;
  overrides: Record<string, DirOverride>;
}

export interface AppInfo {
  version: string;
  os: string;
  config_path: string;
  claude_bin: string | null;
  repo: string;
}

export interface UpdateView {
  channel: string;
  checking: boolean;
  installing: boolean;
  waiting: boolean;
  last_checked: number | null;
  available: { version: string; notes: string | null; date: string | null } | null;
  error: string | null;
  /** Set when this build never checks or installs (a dev build). */
  off: string | null;
}

export interface Permission {
  id: string;
  name: string;
  why: string;
  state: "granted" | "denied" | "not_asked" | "unknown";
  can_prompt: boolean;
  important: boolean;
}

export interface LoginView {
  running: boolean;
  lines: string[];
  exit: string | null;
  success: boolean;
}

export type DirAction = "start" | "restart" | "stop";

export const api = {
  status: () => invoke<Snapshot>("get_status"),
  tail: (key: string, lines = 500) => invoke<string[]>("get_tail", { key, lines }),
  dirAction: (key: string, action: DirAction) => invoke<void>("dir_action", { key, action }),
  answer: (key: string, text: string) => invoke<void>("answer", { key, text }),
  answerAll: (attention: Attention, text: string) => invoke<void>("answer_all", { attention, text }),
  restartAll: () => invoke<void>("restart_all"),
  restartDaemon: () => invoke<void>("restart_daemon"),
  uninstall: () => invoke<void>("uninstall"),
  getConfig: () => invoke<Config>("get_config"),
  configMtime: () => invoke<number | null>("config_mtime"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  pickFolder: () => invoke<string | null>("pick_folder"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  appInfo: () => invoke<AppInfo>("app_info"),
  listSubdirs: (path: string) => invoke<string[]>("list_subdirs", { path }),
  permissions: () => invoke<Permission[]>("get_permissions"),
  requestPermission: (id: string) => invoke<void>("request_permission", { id }),
  completeSetup: (root: string, autostart: boolean) => invoke<void>("complete_setup", { root, autostart }),
  loginStart: () => invoke<void>("login_start"),
  loginView: () => invoke<LoginView>("login_view"),
  loginInput: (text: string) => invoke<void>("login_input", { text }),
  loginCancel: () => invoke<void>("login_cancel"),
  getUpdate: () => invoke<UpdateView>("get_update"),
  checkUpdate: () => invoke<unknown>("check_update"),
  installUpdate: (force: boolean) => invoke<void>("install_update", { force }),
};

export const PERMISSION_MODES = ["acceptEdits", "auto", "bypassPermissions", "default", "dontAsk", "plan"];
export const SPAWN_MODES = ["same-dir", "worktree", "session"];
