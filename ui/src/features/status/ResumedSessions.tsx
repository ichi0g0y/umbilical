// Web sessions that Umbilical keeps, and the ones it started again after a restart.
import { StatusDot } from "@/components/StatusDot";
import { Button } from "@/components/ui/button";
import { api, type DirStatus, type ResumedSession } from "@/lib/api";
import { ago, plural, type Tone } from "@/lib/format";

function stateOf(r: ResumedSession): { tone: Tone; text: string } {
  switch (r.state) {
    case "running":
      return { tone: "ok", text: r.started_at ? `running for ${ago(r.started_at)}` : "running" };
    case "waiting": {
      const left = r.next_restart_at ? Math.max(0, r.next_restart_at - Math.floor(Date.now() / 1000)) : 0;
      return { tone: r.tries ? "bad" : "wait", text: r.tries ? `try ${r.tries + 1} in ${left}s` : "starting" };
    }
    default:
      return { tone: "off", text: "ended" };
  }
}

export function ResumedSessions({ dir: d }: { dir: DirStatus }) {
  if (!d.kept_sessions && !d.resumed.length && !d.resume_note) return null;
  return (
    <div className="flex flex-col gap-1.5">
      <h3 className="font-semibold">Web sessions</h3>
      <p className="text-muted-foreground">{plural(d.kept_sessions, "session")} kept. After a restart, Umbilical starts them again in their worktree.</p>
      {d.resumed.map((r) => {
        const s = stateOf(r);
        return (
          <div key={r.id} className="flex flex-col gap-1 rounded-md border px-3 py-2">
            <div className="flex items-center gap-2">
              <StatusDot tone={s.tone} className="size-2" />
              <code className="text-xs">{r.id}</code>
              <span className="text-muted-foreground">
                resumed · {s.text}
                {r.pid ? `  (pid ${r.pid})` : ""}
              </span>
              <span className="flex-1" />
              {r.session_url && (
                <Button size="sm" variant="ghost" onClick={() => api.openPath(r.session_url!)}>
                  Open
                </Button>
              )}
              <Button size="sm" variant="ghost" onClick={() => api.revealPath(r.log_path)}>
                Log
              </Button>
            </div>
            {r.last_error && r.state !== "running" && <p className="font-mono text-xs break-all text-muted-foreground">{r.last_error}</p>}
          </div>
        );
      })}
      {d.resume_note && <p className="text-xs text-destructive">{d.resume_note}</p>}
    </div>
  );
}
