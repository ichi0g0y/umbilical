import { useAtomValue } from "jotai";
import { RotateCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api } from "@/lib/api";
import { permissionsAtom, snapshotAtom } from "@/state/atoms";

export function Header() {
  const s = useAtomValue(snapshotAtom);
  const permissions = useAtomValue(permissionsAtom);
  const missing = permissions.some((p) => p.important && p.state !== "granted");
  const busy = s.dirs.reduce((n, d) => n + d.busy, 0);

  return (
    <header className="flex items-center gap-4 border-b bg-card px-4 py-2">
      <TabsList>
        <TabsTrigger value="status">Status</TabsTrigger>
        <TabsTrigger value="settings">Settings</TabsTrigger>
        {permissions.length > 0 && (
          <TabsTrigger value="permissions" className="gap-1.5">
            Permissions
            {missing && <span aria-label="missing" className="size-1.5 rounded-full bg-bad" />}
          </TabsTrigger>
        )}
        <TabsTrigger value="about">About</TabsTrigger>
      </TabsList>
      <div className="ml-auto flex items-center gap-3 text-muted-foreground">
        <span>
          {s.running} of {s.total} running{busy > 0 && ` · ${busy} working`}
        </span>
        <Button size="sm" variant="outline" onClick={() => api.restartAll()}>
          <RotateCw />
          Restart all
        </Button>
      </div>
    </header>
  );
}
