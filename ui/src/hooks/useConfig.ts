import { useAtomValue, useSetAtom, useStore } from "jotai";
import { useCallback, useEffect } from "react";
import { api, type Config } from "@/lib/api";
import {
  configLoadErrorAtom,
  configRevisionAtom,
  diskChangedAtom,
  dirtyAtom,
  draftConfigAtom,
  loadedMtimeAtom,
  readyAtom,
  savedConfigAtom,
  snapshotAtom,
} from "@/state/atoms";

/** Trim the text fields only when saving, so a space can be typed in them. */
function tidy(config: Config): Config {
  const c = structuredClone(config);
  c.claude_bin = c.claude_bin.trim();
  c.log_dir = c.log_dir.trim();
  for (const [key, o] of Object.entries(c.overrides)) {
    if (o.name != null) o.name = o.name.trim() || null;
    if (o.claude_bin != null) o.claude_bin = o.claude_bin.trim() || null;
    if (o.name == null) delete o.name;
    if (o.claude_bin == null) delete o.claude_bin;
    if (Object.keys(o).length === 0) delete c.overrides[key];
  }
  return c;
}

export function useConfigActions() {
  const store = useStore();

  /** Replace the draft with the file on disk. On an error, keep the draft and remember the error. */
  const load = useCallback(async (): Promise<Config | null> => {
    try {
      const [config, mtime] = await Promise.all([api.getConfig(), api.configMtime()]);
      store.set(savedConfigAtom, config);
      store.set(draftConfigAtom, structuredClone(config));
      store.set(loadedMtimeAtom, mtime);
      store.set(configLoadErrorAtom, null);
      store.set(configRevisionAtom, (r) => r + 1);
      return config;
    } catch (e) {
      store.set(configLoadErrorAtom, String(e));
      return null;
    }
  }, [store]);

  const save = useCallback(async () => {
    const draft = store.get(draftConfigAtom);
    if (!draft) return;
    const clean = tidy(draft);
    await api.saveConfig(clean);
    store.set(savedConfigAtom, clean);
    store.set(draftConfigAtom, structuredClone(clean));
    store.set(loadedMtimeAtom, await api.configMtime().catch(() => null));
  }, [store]);

  return { load, save };
}

/** Load the config when it can be loaded (also after the first setup), and
 * again when the file changes on disk while nothing is unsaved. With unsaved
 * changes, the save bar warns instead (see `diskChangedAtom`). */
export function useConfigSync() {
  const ready = useAtomValue(readyAtom);
  const snapshot = useAtomValue(snapshotAtom);
  const diskChanged = useAtomValue(diskChangedAtom);
  const dirty = useAtomValue(dirtyAtom);
  const store = useStore();
  const { load } = useConfigActions();

  useEffect(() => {
    if (!ready || snapshot.needs_setup) return;
    if (!store.get(draftConfigAtom) || (diskChanged && !dirty)) void load();
  }, [ready, snapshot.needs_setup, snapshot.config_mtime, snapshot.config_error, diskChanged, dirty, store, load]);
}

/** Change the draft config with a function. */
export function useEditConfig() {
  const setDraft = useSetAtom(draftConfigAtom);
  return useCallback(
    (change: (c: Config) => void) =>
      setDraft((c) => {
        if (!c) return c;
        const next = structuredClone(c);
        change(next);
        return next;
      }),
    [setDraft],
  );
}

export function useDraftConfig(): Config | null {
  return useAtomValue(draftConfigAtom);
}
