import { useAtomValue } from "jotai";
import { useState } from "react";
import { StatusDot } from "@/components/StatusDot";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { api, type Permission } from "@/lib/api";
import { permissionsAtom } from "@/state/atoms";

const STATE: Record<Permission["state"], string> = {
  granted: "Allowed",
  denied: "Not allowed",
  not_asked: "Not asked yet",
  unknown: "See System Settings",
};

function Row({ p }: { p: Permission }) {
  const [busy, setBusy] = useState(false);
  const tone = p.state === "granted" ? "ok" : p.important ? "bad" : "off";
  const label = p.can_prompt && p.state !== "denied" ? "Allow…" : "Open System Settings";
  const request = async () => {
    setBusy(true);
    await api.requestPermission(p.id).catch(() => {});
    setTimeout(() => setBusy(false), 1500);
  };
  return (
    <div className="flex items-start gap-3 py-2.5">
      <StatusDot tone={tone} className="mt-1" />
      <div className="flex-1">
        <div>
          <span className="font-medium">{p.name}</span> <span className="text-muted-foreground">{STATE[p.state]}</span>
        </div>
        <p className="text-xs text-muted-foreground">{p.why}</p>
      </div>
      {p.state !== "granted" && (
        <Button size="sm" variant="outline" disabled={busy} onClick={request}>
          {label}
        </Button>
      )}
    </div>
  );
}

export function PermissionsView() {
  const list = useAtomValue(permissionsAtom);
  return (
    <div className="mx-auto max-w-3xl p-4">
      <Card className="gap-2 py-4">
        <CardHeader className="px-4">
          <CardTitle className="text-sm">Permissions</CardTitle>
          <CardDescription>
            Claude sessions run inside Umbilical, so macOS uses Umbilical's permissions for them. Allow what your sessions need, for example Accessibility and
            Screen Recording for screen control. After a change, use <b>Restart all</b>, so running sessions get it.
          </CardDescription>
        </CardHeader>
        <CardContent className="divide-y px-4">
          {list.map((p) => (
            <Row key={p.id} p={p} />
          ))}
        </CardContent>
      </Card>
    </div>
  );
}
