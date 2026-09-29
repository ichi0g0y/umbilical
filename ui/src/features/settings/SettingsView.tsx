import { useAtomValue } from "jotai";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useConfigActions, useDraftConfig } from "@/hooks/useConfig";
import { appInfoAtom, dirtyAtom } from "@/state/atoms";
import { GeneralCard, LogsCard, RestartCard, UpdatesCard } from "./AppCards";
import { DefaultsCard } from "./DefaultsCard";
import { OverridesCard } from "./OverridesCard";
import { TargetsCard } from "./TargetsCard";

/** The sections in page order. The ids are set on the cards. */
const SECTIONS = [
  { id: "general", title: "General" },
  { id: "updates", title: "Updates" },
  { id: "targets", title: "Targets" },
  { id: "defaults", title: "Defaults" },
  { id: "folders", title: "Per-folder" },
  { id: "restart", title: "Restart and scan" },
  { id: "logs", title: "Logs" },
];

/** The section at the top of the scroll area (the last one when at the bottom).
 * A click on the sidebar keeps its section active while the page scrolls. */
function useActiveSection(scroller: React.RefObject<HTMLDivElement | null>, ready: boolean) {
  const [active, setActive] = useState(SECTIONS[0].id);
  const pinnedUntil = useRef(0);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const update = () => {
      if (Date.now() < pinnedUntil.current) return;
      if (el.scrollTop + el.clientHeight >= el.scrollHeight - 4) return setActive(SECTIONS[SECTIONS.length - 1].id);
      const top = el.getBoundingClientRect().top + 48;
      let current = SECTIONS[0].id;
      for (const s of SECTIONS) {
        const node = document.getElementById(s.id);
        if (node && node.getBoundingClientRect().top <= top) current = s.id;
      }
      setActive(current);
    };
    update();
    el.addEventListener("scroll", update, { passive: true });
    return () => el.removeEventListener("scroll", update);
  }, [scroller, ready]);
  const jump = (id: string) => {
    pinnedUntil.current = Date.now() + 1000;
    setActive(id);
    document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  return [active, jump] as const;
}

export function SettingsView() {
  const config = useDraftConfig();
  const scroller = useRef<HTMLDivElement>(null);
  const [active, jump] = useActiveSection(scroller, !!config);

  if (!config) return <p className="p-6 text-muted-foreground">Loading settings…</p>;

  return (
    <div className="flex h-full">
      <nav className="flex w-44 shrink-0 flex-col gap-0.5 border-r bg-card p-2">
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            type="button"
            onClick={() => jump(s.id)}
            className={cn(
              "rounded-md px-3 py-1.5 text-left text-muted-foreground hover:bg-accent/60 hover:text-foreground",
              active === s.id && "bg-accent font-medium text-foreground",
            )}
          >
            {s.title}
          </button>
        ))}
      </nav>
      <div ref={scroller} className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-3xl flex-col gap-4 p-4">
          <GeneralCard />
          <UpdatesCard />
          <TargetsCard />
          <DefaultsCard />
          <OverridesCard />
          <RestartCard />
          <LogsCard />
        </div>
        <SaveBar />
      </div>
    </div>
  );
}

function SaveBar() {
  const dirty = useAtomValue(dirtyAtom);
  const info = useAtomValue(appInfoAtom);
  const { load, save } = useConfigActions();
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);

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
  );
}
