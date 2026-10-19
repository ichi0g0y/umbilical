import { useAtomValue } from "jotai";
import { RotateCw } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { snapshotAtom } from "@/state/atoms";

/** Stop the daemon with every session and start a new one, after a confirm. */
export function RestartDaemonButton() {
  const s = useAtomValue(snapshotAtom);
  const busy = s.dirs.reduce((n, d) => n + d.busy, 0);
  const [open, setOpen] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const restart = async () => {
    setRunning(true);
    setError(null);
    try {
      await api.restartDaemon();
      setOpen(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <>
      <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
        <RotateCw />
        Restart daemon
      </Button>
      <Dialog open={open} onOpenChange={(v) => !running && setOpen(v)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Restart the daemon?</DialogTitle>
            <DialogDescription>
              This stops every session and the daemon, and starts them again. Then the sessions get the new permissions.
              {busy > 0 && (
                <>
                  {" "}
                  <b>
                    {busy} {busy === 1 ? "session is" : "sessions are"} working now. Their turns stop.
                  </b>
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          {error && <p className="text-sm text-bad">{error}</p>}
          <DialogFooter>
            <Button variant="outline" disabled={running} onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button disabled={running} onClick={restart}>
              {running ? "Restarting…" : "Restart daemon"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
