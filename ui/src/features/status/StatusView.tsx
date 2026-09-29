import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { StatusDot } from "@/components/StatusDot";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { dirTone, stateText } from "@/lib/format";
import { cn } from "@/lib/utils";
import { savedConfigAtom, selectedDirAtom, selectedKeyAtom, snapshotAtom, tabAtom } from "@/state/atoms";
import { DirDetail } from "./DirDetail";

export function StatusView() {
  const s = useAtomValue(snapshotAtom);
  const selected = useAtomValue(selectedDirAtom);
  const [, setKey] = useAtom(selectedKeyAtom);

  if (s.dirs.length === 0) return <Empty />;
  return (
    <div className="flex h-full">
      <ScrollArea className="w-64 shrink-0 border-r bg-card">
        <div className="flex flex-col p-1.5">
          {s.dirs.map((d) => (
            <button
              key={d.key}
              type="button"
              onClick={() => setKey(d.key)}
              className={cn(
                "grid grid-cols-[auto_1fr] items-center gap-x-2.5 rounded-md px-2.5 py-2 text-left hover:bg-accent/60",
                selected?.key === d.key && "bg-accent",
              )}
            >
              <StatusDot tone={dirTone(d)} />
              <span className="truncate font-medium">{d.name}</span>
              <span />
              <span className="truncate text-xs text-muted-foreground">{stateText(d)}</span>
            </button>
          ))}
        </div>
      </ScrollArea>
      <div className="min-w-0 flex-1">{selected && <DirDetail key={selected.key} dir={selected} />}</div>
    </div>
  );
}

function Empty() {
  const config = useAtomValue(savedConfigAtom);
  const setTab = useSetAtom(tabAtom);
  return (
    <div className="mx-auto mt-16 max-w-md space-y-3 text-center">
      <h2 className="text-base font-semibold">No folders yet</h2>
      <p>Put project folders (or symlinks to them) in a root folder, or add folders in Settings.</p>
      {config && <p className="text-xs text-muted-foreground">Roots: {config.roots.join(", ")}</p>}
      <Button variant="outline" size="sm" onClick={() => setTab("settings")}>
        Open Settings
      </Button>
    </div>
  );
}
