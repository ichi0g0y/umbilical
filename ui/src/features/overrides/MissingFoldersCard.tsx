// Overrides whose folder is not there any more. Without this, such a setting
// would be in the file with no way to see or remove it.
import { useAtomValue } from "jotai";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Config, Snapshot } from "@/lib/api";
import { useDraftConfig, useEditConfig } from "@/hooks/useConfig";
import { snapshotAtom } from "@/state/atoms";
import { Section } from "@/features/settings/Section";

/** Keys with an override that no folder uses now. */
export function missingOverrideKeys(config: Config | null, snapshot: Snapshot): string[] {
  if (!config) return [];
  return Object.keys(config.overrides)
    .filter((k) => !snapshot.dirs.some((d) => d.key === k))
    .sort();
}

export function MissingFoldersCard() {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  const snapshot = useAtomValue(snapshotAtom);
  const keys = missingOverrideKeys(config, snapshot);

  return (
    <Section
      id="missing-folders"
      title="Missing folders"
      description="These folders have their own settings, but Umbilical cannot see them now. Open a folder in the Status tab to change its settings."
    >
      <div className="flex flex-col gap-1.5">
        {keys.map((k) => (
          <div key={k} className="flex items-center gap-2 rounded-lg bg-muted/60 py-1 pr-1 pl-3">
            <span className="flex-1 truncate font-mono text-xs">{k}</span>
            <Button
              size="icon"
              variant="ghost"
              className="size-6"
              aria-label={`Remove settings of ${k}`}
              onClick={() => edit((c) => void delete c.overrides[k])}
            >
              <X />
            </Button>
          </div>
        ))}
      </div>
    </Section>
  );
}
