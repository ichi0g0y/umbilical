// What a folder waits for, and how to fix it.
import { useSetAtom } from "jotai";
import { useState } from "react";
import { StatusDot } from "@/components/StatusDot";
import { Button } from "@/components/ui/button";
import { api, type DirStatus } from "@/lib/api";
import { loginOpenAtom } from "@/state/atoms";

export function AttentionNote({ dir }: { dir: DirStatus }) {
  const openLogin = useSetAtom(loginOpenAtom);
  // Answer once: a second click would type a second answer into the session.
  const [answered, setAnswered] = useState(false);
  const answer = (text: string) => {
    setAnswered(true);
    api.answer(dir.key, text);
  };

  let body: React.ReactNode;
  switch (dir.attention) {
    case "trust_prompt":
      body = (
        <>
          The workspace trust dialog is waiting. Turn on <b>Trust every target folder</b> in Settings (it is on by default), or run <code>claude</code> once in
          this folder, then restart.
        </>
      );
      break;
    case "bypass_prompt":
      body = (
        <>
          The bypass permissions warning is waiting. Set <code>"skipDangerousModePermissionPrompt": true</code> in <code>~/.claude/settings.json</code>, then
          restart.
        </>
      );
      break;
    case "remote_control_consent":
      body = (
        <>
          claude asks <b>Enable Remote Control? (y/n)</b>. The session does not show on claude.ai until you answer.
          <span className="ml-2 inline-flex gap-1.5">
            <Button size="xs" disabled={answered} onClick={() => answer("y")}>
              Enable
            </Button>
            <Button size="xs" variant="outline" disabled={answered} onClick={() => answer("n")}>
              No
            </Button>
          </span>
        </>
      );
      break;
    case "login_required":
      body = (
        <>
          claude is not logged in, or the login expired. Log in from Umbilical, or run <code>claude auth login</code> in a terminal, then restart.
          <Button size="xs" className="ml-2" onClick={() => openLogin(true)}>
            Log in…
          </Button>
        </>
      );
      break;
    default:
      body = dir.attention;
  }
  return (
    <div className="flex items-start gap-2 rounded-md border bg-accent/50 px-3 py-2">
      <StatusDot tone="bad" className="mt-1 size-2" />
      <div className="flex-1">{body}</div>
    </div>
  );
}
