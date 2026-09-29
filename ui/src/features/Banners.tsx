// Small notes under the header. Errors are a red dot and one line, no big red boxes.
import { useAtomValue, useSetAtom } from "jotai";
import { useState } from "react";
import { StatusDot } from "@/components/StatusDot";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { plural } from "@/lib/format";
import { loginOpenAtom, snapshotAtom } from "@/state/atoms";

function Bar({ children }: { children: React.ReactNode }) {
  return <div className="flex items-center gap-3 border-b bg-accent/60 px-4 py-2">{children}</div>;
}

export function Banners() {
  const s = useAtomValue(snapshotAtom);
  const openLogin = useSetAtom(loginOpenAtom);
  const [answered, setAnswered] = useState(false);

  const problems: string[] = [];
  if (s.daemon_error) problems.push(`Cannot reach the Umbilical daemon: ${s.daemon_error}`);
  if (s.config_error) problems.push(`Config error (old settings still used): ${s.config_error}`);
  if (s.dirs.length && !s.claude_bin) problems.push("claude binary not found. Set it in Settings.");

  const consent = s.dirs.filter((d) => d.attention === "remote_control_consent").length;
  const login = s.dirs.filter((d) => d.attention === "login_required").length;

  // Answer once: a second click would type a second "y" into the session.
  const enableAll = () => {
    setAnswered(true);
    api.answerAll("remote_control_consent", "y");
    setTimeout(() => setAnswered(false), 5000);
  };

  return (
    <>
      {problems.map((p) => (
        <div key={p} className="flex items-center gap-2 border-b px-4 py-1.5 text-xs">
          <StatusDot tone="bad" className="size-2" />
          <span className="break-all">{p}</span>
        </div>
      ))}
      {consent > 0 && (
        <Bar>
          <span className="flex-1">
            <b>Remote Control is waiting for your OK</b> in {plural(consent, "folder")}. claude asks "Enable Remote Control? (y/n)".
          </span>
          <Button size="sm" disabled={answered} onClick={enableAll}>
            Enable all
          </Button>
        </Bar>
      )}
      {login > 0 && (
        <Bar>
          <span className="flex-1">
            <b>claude is not logged in</b> for {plural(login, "folder")}. Sessions cannot start until you log in.
          </span>
          <Button size="sm" onClick={() => openLogin(true)}>
            Log in…
          </Button>
        </Bar>
      )}
    </>
  );
}
