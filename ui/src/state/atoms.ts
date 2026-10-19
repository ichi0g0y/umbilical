// App state (Jotai). The Rust side pushes the status every second.
import { atom } from "jotai";
import type { AppInfo, Config, Permission, Snapshot, UpdateView } from "@/lib/api";

export type Tab = "status" | "settings" | "permissions" | "about";

export const emptySnapshot: Snapshot = {
  dirs: [],
  running: 0,
  total: 0,
  config_path: "",
  config_error: null,
  config_held: false,
  log_dir: "",
  claude_bin: null,
  needs_setup: false,
  daemon_error: null,
  config_mtime: null,
};

export const snapshotAtom = atom<Snapshot>(emptySnapshot);
/** false until the first status arrives. */
export const readyAtom = atom(false);
export const tabAtom = atom<Tab>("status");

/** The folder shown in the status view. Falls back to the first one. */
export const selectedKeyAtom = atom<string | null>(null);
export const selectedDirAtom = atom((get) => {
  const dirs = get(snapshotAtom).dirs;
  const key = get(selectedKeyAtom);
  return dirs.find((d) => d.key === key) ?? dirs[0] ?? null;
});

/** Last loaded or saved config, and the copy the settings screen edits. */
export const savedConfigAtom = atom<Config | null>(null);
export const draftConfigAtom = atom<Config | null>(null);
/** Changes when the draft is replaced from outside (load, revert): forms reset their text. */
export const configRevisionAtom = atom(0);
/** Why the config could not be loaded, or null. */
export const configLoadErrorAtom = atom<string | null>(null);
/** The file time when the config was loaded or saved. */
export const loadedMtimeAtom = atom<number | null>(null);
/** The file changed on disk after we loaded it (a hand edit, or another app). */
export const diskChangedAtom = atom((get) => {
  const now = get(snapshotAtom).config_mtime;
  const loaded = get(loadedMtimeAtom);
  return now != null && loaded != null && now !== loaded;
});
export const dirtyAtom = atom((get) => {
  const saved = get(savedConfigAtom);
  const draft = get(draftConfigAtom);
  return !!saved && !!draft && JSON.stringify(saved) !== JSON.stringify(draft);
});

export const updateAtom = atom<UpdateView | null>(null);
export const appInfoAtom = atom<AppInfo | null>(null);
export const permissionsAtom = atom<Permission[]>([]);
export const loginOpenAtom = atom(false);
