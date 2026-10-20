import { useAtomValue } from "jotai";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { appInfoAtom } from "@/state/atoms";
import { Section } from "./Section";

/** The same uninstall as "Uninstall Umbilical…" in the menus. The app asks
 * first in its own dialog. On Windows it starts the uninstaller. */
export function UninstallCard() {
  const info = useAtomValue(appInfoAtom);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const windows = info?.os === "windows";
  const onClick = async () => {
    setError(null);
    setBusy(true);
    try {
      await api.uninstall();
    } catch (e) {
      setError(String(e));
    }
    // A second click must not start a second uninstaller.
    setTimeout(() => setBusy(false), 5000);
  };
  return (
    <Section
      id="uninstall"
      title="Uninstall"
      description={
        windows
          ? "Opens the Windows uninstaller. It stops every session and the daemon and turns off start at login."
          : `Stops every session and the daemon, turns off start at login${info?.os === "macos" ? ", moves Umbilical.app to the Trash" : ""} and quits. You choose to keep or delete the settings and logs. Your folders and repositories stay.`
      }
    >
      <div className="flex items-center gap-3">
        <Button size="sm" variant="outline" disabled={busy} onClick={() => void onClick()}>
          Uninstall Umbilical…
        </Button>
        {error && <span className="text-destructive">{error}</span>}
      </div>
    </Section>
  );
}
