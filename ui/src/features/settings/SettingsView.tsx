import { useAtomValue } from "jotai";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { MissingFoldersCard, missingOverrideKeys } from "@/features/overrides/MissingFoldersCard";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useConfigActions, useDraftConfig } from "@/hooks/useConfig";
import { appInfoAtom, configLoadErrorAtom, diskChangedAtom, dirtyAtom, snapshotAtom } from "@/state/atoms";
import { GeneralCard, LogsCard, RestartCard, UpdatesCard } from "./AppCards";
import { DefaultsCard } from "./DefaultsCard";
import { TargetsCard } from "./TargetsCard";

/** The sections in page order, in two groups. The ids are set on the cards.
 * A section's nav label, its card title and its id all use the same words.
 * "Missing folders" is only there when a folder with its own settings is gone. */
function groupsFor(missing: boolean) {
  return [
    {
      title: "Folders",
      sections: [
        { id: "targets", title: "Targets" },
        { id: "defaults", title: "Defaults" },
        ...(missing ? [{ id: "missing-folders", title: "Missing folders" }] : []),
        { id: "restart", title: "Restart and scan" },
      ],
    },
    {
      title: "App",
      sections: [
        { id: "general", title: "General" },
        { id: "updates", title: "Updates" },
        { id: "logs", title: "Logs" },
      ],
    },
  ];
}

type SectionRef = { id: string; title: string };

/** The section at the top of the scroll area (the last one when at the bottom).
 * A click on the sidebar keeps its section active while the page scrolls. */
function useActiveSection(scroller: React.RefObject<HTMLDivElement | null>, sections: SectionRef[], ready: boolean) {
  const [active, setActive] = useState(sections[0].id);
  const pinnedUntil = useRef(0);
  const list = useRef(sections);
  list.current = sections;
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const update = () => {
      if (Date.now() < pinnedUntil.current) return;
      const all = list.current;
      if (el.scrollTop + el.clientHeight >= el.scrollHeight - 4) return setActive(all[all.length - 1].id);
      const top = el.getBoundingClientRect().top + 48;
      let current = all[0].id;
      for (const s of all) {
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
  const snapshot = useAtomValue(snapshotAtom);
  const missing = missingOverrideKeys(config, snapshot).length > 0;
  const groups = groupsFor(missing);
  const scroller = useRef<HTMLDivElement>(null);
  const [active, jump] = useActiveSection(
    scroller,
    groups.flatMap((g) => g.sections),
    !!config,
  );

  if (!config) return <NotLoaded />;

  return (
    <div className="flex h-full">
      <nav className="flex w-44 shrink-0 flex-col gap-3 border-r border-border/50 bg-card/60 p-2">
        {groups.map((g) => (
          <div key={g.title} className="flex flex-col gap-0.5">
            <span className="px-3 pb-0.5 text-xs font-medium text-muted-foreground/80">{g.title}</span>
            {g.sections.map((s) => (
              <button
                key={s.id}
                type="button"
                onClick={() => jump(s.id)}
                className={cn(
                  "rounded-lg px-3 py-1.5 text-left text-muted-foreground hover:bg-accent/60 hover:text-foreground",
                  active === s.id && "bg-accent font-medium text-foreground",
                )}
              >
                {s.title}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <div ref={scroller} className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-3xl flex-col gap-4 p-4">
          <TargetsCard />
          <DefaultsCard />
          {missing && <MissingFoldersCard />}
          <RestartCard />
          <GeneralCard />
          <UpdatesCard />
          <LogsCard />
        </div>
        <SaveBar />
      </div>
    </div>
  );
}

/** The config is not loaded yet, or it cannot be read. */
function NotLoaded() {
  const error = useAtomValue(configLoadErrorAtom);
  const info = useAtomValue(appInfoAtom);
  const { load } = useConfigActions();
  if (!error) return <p className="p-6 text-muted-foreground">Loading settings…</p>;
  return (
    <div className="mx-auto mt-12 flex max-w-lg flex-col gap-3 p-6">
      <p className="font-medium">The config file cannot be read.</p>
      <p className="font-mono text-xs break-all text-muted-foreground">{error}</p>
      <div className="flex gap-2">
        <Button variant="outline" onClick={() => void load()}>
          Try again
        </Button>
        {info && (
          <Button variant="outline" onClick={() => api.revealPath(info.config_path)}>
            Show config file
          </Button>
        )}
      </div>
    </div>
  );
}

function SaveBar() {
  const dirty = useAtomValue(dirtyAtom);
  const diskChanged = useAtomValue(diskChangedAtom);
  const info = useAtomValue(appInfoAtom);
  const { load, save } = useConfigActions();
  const [msg, setMsg] = useState<{ text: string; error?: boolean } | null>(null);
  // A new edit makes an old "Saved" message wrong.
  useEffect(() => {
    if (dirty) setMsg(null);
  }, [dirty]);
  const conflict = dirty && diskChanged;

  const onSave = async () => {
    try {
      await save();
      setMsg({ text: `Saved at ${new Date().toLocaleTimeString()}. Changed folders restart now.` });
    } catch (e) {
      setMsg({ text: String(e), error: true });
    }
  };
  const onRevert = async () => {
    const config = await load();
    setMsg(config ? null : { text: "Cannot read the config file. See the message above, or Show config file.", error: true });
  };

  return (
    <div className="sticky bottom-0 border-t bg-background/95 backdrop-blur">
      <div className="mx-auto flex max-w-3xl items-center gap-2 px-4 py-3">
        <span className={msg?.error ? "flex-1 text-destructive" : "flex-1 text-muted-foreground"}>
          {msg?.text ?? (conflict ? "The config file changed on disk. Save overwrites those changes; Revert loads them." : dirty ? "Not saved yet." : "")}
        </span>
        {info && (
          <Button variant="outline" onClick={() => api.revealPath(info.config_path)}>
            Show config file
          </Button>
        )}
        <Button variant="outline" onClick={onRevert}>
          Revert
        </Button>
        <Button disabled={!dirty} variant={conflict ? "destructive" : "default"} onClick={onSave}>
          {conflict ? "Save anyway" : "Save"}
        </Button>
      </div>
    </div>
  );
}
