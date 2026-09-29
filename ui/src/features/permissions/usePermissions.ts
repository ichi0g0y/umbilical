import { useAtomValue, useSetAtom } from "jotai";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { permissionsAtom, snapshotAtom, tabAtom } from "@/state/atoms";

/** Poll the macOS permissions. The list is empty on other platforms. */
export function usePermissions() {
  const setPermissions = useSetAtom(permissionsAtom);
  const setTab = useSetAtom(tabAtom);
  const needsSetup = useAtomValue(snapshotAtom).needs_setup;

  useEffect(() => {
    let alive = true;
    const load = async () => {
      const list = await api.permissions().catch(() => null);
      if (!alive || !list) return;
      setPermissions(list);
      const missing = list.some((p) => p.important && p.state !== "granted");
      if (missing && !needsSetup) showOnce(() => setTab("permissions"));
    };
    load();
    const timer = setInterval(load, 3000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [setPermissions, setTab, needsSetup]);
}

/** First start: open the Permissions tab once, when something important is missing. */
function showOnce(show: () => void) {
  try {
    if (localStorage.getItem("permissionsShown")) return;
    localStorage.setItem("permissionsShown", "1");
  } catch {
    return;
  }
  show();
}
