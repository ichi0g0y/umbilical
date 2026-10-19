import { ChevronRight, Play, RotateCw, Square } from "lucide-react";
import { type ReactNode, useState } from "react";
import { StatusDot } from "@/components/StatusDot";
import { Button } from "@/components/ui/button";
import { FolderSettingsDialog } from "@/features/overrides/FolderSettingsDialog";
import { api, type DirAction, type DirStatus } from "@/lib/api";
import { stateText } from "@/lib/format";
import { AttentionNote } from "./AttentionNote";
import { LogView } from "./LogView";
import { ResumedSessions } from "./ResumedSessions";

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
    <Button size="sm" variant="outline" disabled={disabled} onClick={() => api.dirAction(dir.key, action)}>
      {children}
      {label}
    </Button>
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
  const [settingsOpen, setSettingsOpen] = useState(false);
  const running = d.state === "running";

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <div className="min-w-0">
        <h2 className="truncate text-base font-semibold">{d.name}</h2>
        <p className="truncate font-mono text-xs text-muted-foreground">{d.path}</p>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <ActionButton dir={d} action="start" label="Start" disabled={running || d.state === "disabled"}>
          <Play />
        </ActionButton>
        <ActionButton dir={d} action="restart" label="Restart" disabled={d.state === "disabled"}>
          <RotateCw />
        </ActionButton>
        <ActionButton dir={d} action="stop" label="Stop" disabled={!running && d.state !== "waiting"}>
          <Square />
        </ActionButton>
        <span aria-hidden className="mx-1 h-5 w-px bg-border" />
        <Button size="sm" variant="outline" onClick={() => api.revealPath(d.log_path)}>
          Open log file
        </Button>
        <Button size="sm" variant="outline" onClick={() => api.openPath(d.key)}>
          Show folder
        </Button>
        <Button size="sm" variant="outline" onClick={() => setSettingsOpen(true)}>
          Settings…
        </Button>
      </div>

      {d.attention && <AttentionNote dir={d} />}
      {!d.attention && d.already_served && (
        <div className="flex items-start gap-2 rounded-md border px-3 py-2 text-muted-foreground">
          <StatusDot tone="wait" className="mt-1 size-2" />
          <div className="flex-1">
            An older <code>claude remote-control</code> is still registered for this folder. The registration ends by itself in a few minutes, and Umbilical
            tries again every 30 seconds. Nothing to do.
          </div>
        </div>
      )}

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
      </dl>

      {/* Open when something went wrong, so an error is never hidden. */}
      <details className="group" open={!!d.last_error && !d.already_served}>
        <summary className="flex w-fit cursor-pointer items-center gap-1 text-muted-foreground hover:text-foreground">
          <ChevronRight className="size-3.5 transition-transform group-open:rotate-90" />
          Details
        </summary>
        <dl className="mt-2 grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1">
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
      </details>

      <ResumedSessions dir={d} />

      <h3 className="font-semibold">Output</h3>
      <LogView dirKey={d.key} />
      <FolderSettingsDialog dir={d} open={settingsOpen} onOpenChange={setSettingsOpen} />
    </div>
  );
}
