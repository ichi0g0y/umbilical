import { useAtomValue } from "jotai";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { api, type UpdateView } from "@/lib/api";
import { ago } from "@/lib/format";
import { updateAtom } from "@/state/atoms";

function statusText(u: UpdateView | null): string {
  if (!u) return "Not checked yet.";
  if (u.installing) return "Installing… the app restarts when done.";
  if (u.waiting) return `Version ${u.available?.version} installs when no session is working. "Install now" stops running turns.`;
  if (u.checking) return "Checking…";
  if (u.error) return `Update check failed: ${u.error}`;
  if (u.available) return `Version ${u.available.version} is available (${u.channel} channel).`;
  if (u.last_checked) return `Up to date (${u.channel} channel). Last check ${ago(u.last_checked)} ago.`;
  return "Not checked yet.";
}

export function UpdateCard() {
  const u = useAtomValue(updateAtom);
  const notes = u?.available?.notes;
  return (
    <Card className="gap-3 py-4">
      <CardHeader className="px-4">
        <CardTitle className="text-sm">Update</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 px-4">
        <p className="text-muted-foreground">{statusText(u)}</p>
        {notes && <pre className="max-h-48 overflow-auto rounded-md border bg-muted p-3 text-xs whitespace-pre-wrap">{notes}</pre>}
        <div className="flex gap-2">
          <Button variant="outline" disabled={u?.checking || u?.installing} onClick={() => api.checkUpdate().catch(() => {})}>
            Check now
          </Button>
          {u?.available && !u.installing && <Button onClick={() => api.installUpdate(u.waiting)}>{u.waiting ? "Install now" : "Install and restart"}</Button>}
        </div>
      </CardContent>
    </Card>
  );
}
