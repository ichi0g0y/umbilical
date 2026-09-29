import { useAtomValue, useSetAtom, useStore } from "jotai";
import { useCallback } from "react";
import { api, type Config } from "@/lib/api";
import { configRevisionAtom, draftConfigAtom, savedConfigAtom } from "@/state/atoms";

export function useConfigActions() {
  const store = useStore();

  /** Replace the draft with the file on disk. */
  const load = useCallback(async () => {
    const config = await api.getConfig();
    store.set(savedConfigAtom, config);
    store.set(draftConfigAtom, structuredClone(config));
    store.set(configRevisionAtom, (r) => r + 1);
    return config;
  }, [store]);

  const save = useCallback(async () => {
    const draft = store.get(draftConfigAtom);
    if (!draft) return;
    await api.saveConfig(draft);
    store.set(savedConfigAtom, structuredClone(draft));
  }, [store]);

  return { load, save };
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
