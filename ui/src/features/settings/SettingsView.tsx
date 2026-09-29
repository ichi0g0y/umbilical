import { useAtomValue } from "jotai";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { useConfigActions, useDraftConfig } from "@/hooks/useConfig";
import { appInfoAtom, dirtyAtom } from "@/state/atoms";
import { AppCard, RestartCard, UpdatesCard } from "./AppCards";
import { DefaultsCard } from "./DefaultsCard";
import { OverridesCard } from "./OverridesCard";
import { TargetsCard } from "./TargetsCard";

export function SettingsView() {
  const config = useDraftConfig();
  const dirty = useAtomValue(dirtyAtom);
  const info = useAtomValue(appInfoAtom);
  const { load, save } = useConfigActions();
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);

  if (!config) return <p className="p-6 text-muted-foreground">Loading settings…</p>;

  const onSave = async () => {
    try {
      await save();
      setMsg({ text: `Saved at ${new Date().toLocaleTimeString()}. Changed folders restart now.` });
    } catch (e) {
      setMsg({ text: String(e), error: true });
    }
  };
  const onRevert = async () => {
    try {
      await load();
      setMsg(null);
    } catch (e) {
      setMsg({ text: `Cannot read config: ${e}`, error: true });
    }
  };

  return (
    <div className="min-h-full">
      <div className="mx-auto flex max-w-3xl flex-col gap-4 p-4">
        <TargetsCard />
        <DefaultsCard />
        <OverridesCard />
        <RestartCard />
        <AppCard />
        <UpdatesCard />
      </div>
      <div className="sticky bottom-0 border-t bg-background/95 backdrop-blur">
        <div className="mx-auto flex max-w-3xl items-center gap-2 px-4 py-3">
          <span className={msg?.error ? "flex-1 text-destructive" : "flex-1 text-muted-foreground"}>{msg?.text ?? (dirty ? "Not saved yet." : "")}</span>
          {info && (
            <Button variant="outline" onClick={() => api.revealPath(info.config_path)}>
              Show config file
            </Button>
          )}
          <Button variant="outline" onClick={onRevert}>
            Revert
          </Button>
          <Button disabled={!dirty} onClick={onSave}>
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}
