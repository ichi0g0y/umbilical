// "Log in…": runs `claude auth login` in the app and shows its output.
import { useAtom } from "jotai";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { api, type LoginView } from "@/lib/api";
import { loginOpenAtom } from "@/state/atoms";

export function LoginDialog() {
  const [open, setOpen] = useAtom(loginOpenAtom);
  const [view, setView] = useState<LoginView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const logRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    let timer: ReturnType<typeof setInterval> | undefined;
    setView(null);
    setError(null);
    api
      .loginStart()
      .then(() => {
        const poll = async () => {
          const v = await api.loginView().catch(() => null);
          if (!alive || !v) return;
          setView(v);
          if (!v.running && v.exit) clearInterval(timer);
        };
        poll();
        timer = setInterval(poll, 500);
      })
      .catch((e) => setError(String(e)));
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [open]);

  const text = (view?.lines ?? []).map((l) => l.replace(/^\S+ \S+ /, "")).join("\n");
  const urls = [...new Set(text.match(/https:\/\/[^\s"'<>]+/g) ?? [])];
  const done = !!view && !view.running && !!view.exit;

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [text]);

  const cancel = async () => {
    await api.loginCancel().catch(() => {});
    setOpen(false);
  };

  return (
    <Dialog open={open} onOpenChange={(o) => (o ? setOpen(true) : done ? setOpen(false) : cancel())}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Log in to Claude</DialogTitle>
          <DialogDescription>
            Umbilical runs <code>claude auth login</code> for you. Your browser should open. Finish the login there. If it asks for a code, paste it below.
          </DialogDescription>
        </DialogHeader>
        {urls.map((u) => (
          <button key={u} type="button" className="text-left text-xs break-all text-primary hover:underline" onClick={() => api.openPath(u)}>
            Open login page: {u}
          </button>
        ))}
        <pre ref={logRef} className="max-h-40 overflow-auto rounded-md border bg-muted p-2 font-mono text-xs whitespace-pre-wrap">
          {text || "Starting…"}
        </pre>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (code.trim()) api.loginInput(code.trim());
            setCode("");
          }}
        >
          <Input value={code} onChange={(e) => setCode(e.target.value)} placeholder="Paste a code here if asked" spellCheck={false} autoComplete="off" />
          <Button type="submit" variant="outline">
            Send
          </Button>
        </form>
        {error && <p className="text-destructive">{error}</p>}
        {done && (
          <p className={view.success ? "text-ok" : "text-destructive"}>
            {view.success ? "Logged in. Restarting all sessions…" : `Login did not finish (${view.exit}).`}
          </p>
        )}
        <DialogFooter>
          {done ? (
            <Button onClick={() => setOpen(false)}>Close</Button>
          ) : (
            <Button variant="outline" onClick={cancel}>
              Cancel
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
