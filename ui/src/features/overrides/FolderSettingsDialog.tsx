// Settings of the folder you are looking at. It edits the same draft config as
// the Settings tab, so Save here is the same Save as there.
import { useAtomValue } from "jotai";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import type { DirStatus } from "@/lib/api";
import { useConfigActions, useDraftConfig } from "@/hooks/useConfig";
import { dirtyAtom } from "@/state/atoms";
import { OverrideFields } from "./OverrideFields";

export function FolderSettingsDialog({ dir, open, onOpenChange }: { dir: DirStatus; open: boolean; onOpenChange: (v: boolean) => void }) {
  const config = useDraftConfig();
  const dirty = useAtomValue(dirtyAtom);
  const { save } = useConfigActions();
  const [error, setError] = useState<string | null>(null);

  const onSave = async () => {
    try {
      await save();
      setError(null);
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{dir.name}</DialogTitle>
          <DialogDescription className="font-mono text-xs break-all">{dir.path}</DialogDescription>
        </DialogHeader>
        {config ? <OverrideFields dirKey={dir.key} /> : <p className="text-muted-foreground">Loading settings…</p>}
        <DialogFooter className="items-center sm:justify-between">
          <span className={error ? "text-destructive" : "text-muted-foreground"}>{error ?? "Saving changes this folder restarts it."}</span>
          <div className="flex gap-2">
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              Close
            </Button>
            <Button disabled={!dirty} onClick={onSave}>
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
