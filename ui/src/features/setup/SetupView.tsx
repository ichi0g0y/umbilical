// First start: choose the root folder.
import { useEffect, useId, useState } from "react";
import { CheckField } from "@/components/fields";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { api } from "@/lib/api";
import { plural } from "@/lib/format";

const DEFAULT_ROOT = "~/.umbilical/repos";

function Preview({ root }: { root: string | null }) {
  const [names, setNames] = useState<string[] | null | "missing">(null);
  useEffect(() => {
    setNames(null);
    if (!root) return;
    api
      .listSubdirs(root)
      .then(setNames)
      .catch(() => setNames("missing"));
  }, [root]);
  if (!root) return <p className="text-muted-foreground">Choose a folder.</p>;
  if (names === null) return null;
  if (names === "missing")
    return <p className="text-muted-foreground">The folder does not exist yet. It will be created. No session starts until you add folders.</p>;
  if (names.length === 0) return <p className="text-muted-foreground">The folder is empty now. No session starts until you add folders.</p>;
  return (
    <div>
      <b>{plural(names.length, "session")} will start:</b>
      <ul className="mt-1 max-h-40 list-disc overflow-auto pl-5">
        {names.map((n) => (
          <li key={n}>{n}</li>
        ))}
      </ul>
    </div>
  );
}

export function SetupView() {
  const [choice, setChoice] = useState<"default" | "custom">("default");
  const [custom, setCustom] = useState<string | null>(null);
  const [autostart, setAutostart] = useState(true);
  const [msg, setMsg] = useState("");
  const [starting, setStarting] = useState(false);
  const ids = { def: useId(), custom: useId() };
  const root = choice === "custom" ? custom : DEFAULT_ROOT;

  const pick = async () => {
    const p = await api.pickFolder();
    if (!p) return;
    setCustom(p);
    setChoice("custom");
  };
  const start = async () => {
    if (!root) return setMsg("Choose a folder first.");
    setStarting(true);
    try {
      await api.completeSetup(root, autostart);
      setMsg("Starting…");
    } catch (e) {
      setMsg(String(e));
      setStarting(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
        <Card className="gap-4 py-5">
          <CardHeader className="px-5">
            <CardTitle className="text-base">Welcome to Umbilical</CardTitle>
            <p>
              Choose the <b>root folder</b>. Every folder directly inside it gets its own <code>claude remote-control</code> session. You can change this later
              in Settings.
            </p>
          </CardHeader>
          <CardContent className="flex flex-col gap-4 px-5">
            <RadioGroup value={choice} onValueChange={(v) => setChoice(v as "default" | "custom")}>
              <div className="flex items-start gap-2">
                <RadioGroupItem id={ids.def} value="default" className="mt-0.5" />
                <Label htmlFor={ids.def} className="flex-col items-start font-normal">
                  <b>
                    Use <code>{DEFAULT_ROOT}</code>
                  </b>
                  <span className="text-muted-foreground">Starts empty. Put folders or symlinks to the projects you want there.</span>
                </Label>
              </div>
              <div className="flex items-start gap-2">
                <RadioGroupItem id={ids.custom} value="custom" className="mt-0.5" />
                <Label htmlFor={ids.custom} className="flex-col items-start font-normal">
                  <b>Use a folder I choose</b>
                  <span className="font-mono text-xs text-muted-foreground">{custom ?? "no folder chosen"}</span>
                </Label>
              </div>
            </RadioGroup>
            <Button size="sm" variant="outline" className="self-start" onClick={pick}>
              Choose folder…
            </Button>
            <div className="rounded-md border bg-muted/50 p-3">
              <Preview root={root} />
            </div>
          </CardContent>
        </Card>
        <Card className="gap-3 border-wait/50 py-5">
          <CardHeader className="px-5">
            <CardTitle className="text-sm">Before you start</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-3 px-5">
            <ul className="list-disc space-y-1 pl-5">
              <li>
                Sessions use the permission mode <code>bypassPermissions</code>. Anyone who can use your Claude account can run commands in these folders
                without asking. Turn on two-factor authentication.
              </li>
              <li>
                Umbilical marks the root folder and every folder in it as trusted in Claude Code, so the trust dialog does not block sessions. You can turn this
                off in Settings.
              </li>
            </ul>
            <CheckField checked={autostart} onChange={setAutostart}>
              Start Umbilical at login
            </CheckField>
          </CardContent>
        </Card>
        <div className="flex items-center justify-end gap-3">
          <span className="text-muted-foreground">{msg}</span>
          <Button disabled={starting} onClick={start}>
            Start
          </Button>
        </div>
      </div>
    </div>
  );
}
