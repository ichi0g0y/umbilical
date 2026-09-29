// Keeps the atoms in sync with the Rust side.
import { listen } from "@tauri-apps/api/event";
import { useSetAtom } from "jotai";
import { useEffect } from "react";
import { api, type Snapshot, type UpdateView } from "@/lib/api";
import { appInfoAtom, readyAtom, snapshotAtom, type Tab, tabAtom, updateAtom } from "@/state/atoms";
import { useConfigActions } from "./useConfig";

export function useBackend() {
  const setSnapshot = useSetAtom(snapshotAtom);
  const setReady = useSetAtom(readyAtom);
  const setUpdate = useSetAtom(updateAtom);
  const setInfo = useSetAtom(appInfoAtom);
  const setTab = useSetAtom(tabAtom);
  const { load } = useConfigActions();

  useEffect(() => {
    let alive = true;
    const onStatus = (s: Snapshot) => {
      if (!alive) return;
      setSnapshot(s);
      setReady(true);
    };
    const unlisten = [
      listen<Snapshot>("status", (e) => onStatus(e.payload)),
      listen<UpdateView>("update", (e) => setUpdate(e.payload)),
      // The tray and app menus open a tab ("About Umbilical", "Check for Updates…").
      listen<Tab>("navigate", (e) => setTab(e.payload)),
    ];
    api
      .status()
      .then(onStatus)
      .catch(() => {});
    api
      .getUpdate()
      .then(setUpdate)
      .catch(() => {});
    api
      .appInfo()
      .then(setInfo)
      .catch(() => {});
    load();
    return () => {
      alive = false;
      for (const u of unlisten) u.then((f) => f());
    };
  }, [setSnapshot, setReady, setUpdate, setInfo, setTab, load]);
}

/** Follow the system light / dark setting. */
export function useSystemTheme() {
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => document.documentElement.classList.toggle("dark", media.matches);
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, []);
}
