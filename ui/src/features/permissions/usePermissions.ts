import { useAtomValue, useSetAtom } from "jotai";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { permissionsAtom, readyAtom, snapshotAtom, tabAtom } from "@/state/atoms";

/** Poll the macOS permissions. The list is empty on other platforms. */
export function usePermissions() {
  const setPermissions = useSetAtom(permissionsAtom);
  const setTab = useSetAtom(tabAtom);
  const tab = useAtomValue(tabAtom);
  const ready = useAtomValue(readyAtom);
  const needsSetup = useAtomValue(snapshotAtom).needs_setup;
  // Each check starts a small process (macOS tells a running app some changes
  // only after a restart), so check often only while the tab is open.
  const every = tab === "permissions" ? 3000 : 15000;

  useEffect(() => {
    let alive = true;
    const load = async () => {
      const list = await api.permissions().catch(() => null);
      if (!alive || !list) return;
      setPermissions(list);
      const missing = list.some((p) => p.important && p.state !== "granted");
      // Only after the first setup: the setup screen comes first.
      if (missing && ready && !needsSetup) showOnce(() => setTab("permissions"));
    };
    load();
    const timer = setInterval(load, every);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [setPermissions, setTab, ready, needsSetup, every]);
}

/** Open the Permissions tab once, when something important is missing. */
function showOnce(show: () => void) {
  try {
    if (localStorage.getItem("permissionsShown")) return;
    localStorage.setItem("permissionsShown", "1");
  } catch {
    return;
  }
  show();
}
