import { useSetAtom } from "jotai";
import { Play, RotateCw, Square } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { api, type DirAction, type DirStatus } from "@/lib/api";
import { stateText } from "@/lib/format";
import { overrideKeyAtom, tabAtom } from "@/state/atoms";
import { AttentionNote } from "./AttentionNote";
import { LogView } from "./LogView";

function ActionButton({
  action,
  label,
  disabled,
  dir,
  children,
}: {
  action: DirAction;
  label: string;
  disabled: boolean;
  dir: DirStatus;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button size="icon" variant="outline" aria-label={label} disabled={disabled} onClick={() => api.dirAction(dir.key, action)}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function Fact({ label, children, mono }: { label: string; children: ReactNode; mono?: boolean }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className={mono ? "font-mono text-xs break-all" : "break-all"}>{children}</dd>
    </>
  );
}

export function DirDetail({ dir: d }: { dir: DirStatus }) {
  const setTab = useSetAtom(tabAtom);
  const setOverrideKey = useSetAtom(overrideKeyAtom);
  const running = d.state === "running";

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-base font-semibold">{d.name}</h2>
          <p className="truncate font-mono text-xs text-muted-foreground">{d.path}</p>
        </div>
        <div className="flex gap-1.5">
          <ActionButton dir={d} action="start" label="Start" disabled={running || d.state === "disabled"}>
            <Play />
          </ActionButton>
          <ActionButton dir={d} action="restart" label="Restart" disabled={d.state === "disabled"}>
            <RotateCw />
          </ActionButton>
          <ActionButton dir={d} action="stop" label="Stop" disabled={!running && d.state !== "waiting"}>
            <Square />
          </ActionButton>
        </div>
      </div>

      {d.attention && <AttentionNote dir={d} />}

      <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1">
        <Fact label="State">
          {stateText(d)}
          {d.pid ? `  (pid ${d.pid})` : ""}
        </Fact>
        <Fact label="Session">
          {d.session_url ? (
            <button type="button" className="text-primary hover:underline" onClick={() => api.openPath(d.session_url!)}>
              {d.session_url}
            </button>
          ) : (
            <span className="text-muted-foreground">not seen yet</span>
          )}
        </Fact>
        <Fact label="Spawn">{d.spawn === d.effective_spawn ? d.spawn : `${d.effective_spawn} (set: ${d.spawn}, no git)`}</Fact>
        <Fact label="Restarts">{d.restarts}</Fact>
        <Fact label="Last exit">{d.last_exit ?? "—"}</Fact>
        <Fact label="Last error" mono>
          {d.last_error ?? "—"}
        </Fact>
        <Fact label="Command" mono>
          {d.command ?? "—"}
        </Fact>
      </dl>

      <div className="flex items-center gap-2">
        <h3 className="flex-1 font-semibold">Output</h3>
        <Button size="sm" variant="outline" onClick={() => api.revealPath(d.log_path)}>
          Open log file
        </Button>
        <Button size="sm" variant="outline" onClick={() => api.openPath(d.key)}>
          Show folder
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            setOverrideKey(d.key);
            setTab("settings");
          }}
        >
          Folder settings
        </Button>
      </div>
      <LogView dirKey={d.key} />
    </div>
  );
}
