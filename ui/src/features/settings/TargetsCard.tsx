import { X } from "lucide-react";
import { CheckField, Field, TextAreaField } from "@/components/fields";
import { Button } from "@/components/ui/button";
import { api, type Config } from "@/lib/api";
import { toLines } from "@/lib/format";
import { useDraftConfig, useEditConfig } from "@/hooks/useConfig";
import { Section } from "./Section";

function PathList({ list }: { list: "roots" | "dirs" }) {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  const items = config[list];
  const add = async () => {
    const p = await api.pickFolder();
    if (p) edit((c: Config) => void (c[list].includes(p) || c[list].push(p)));
  };
  return (
    <div className="flex flex-col gap-1.5">
      {items.length === 0 && <p className="text-xs text-muted-foreground">none</p>}
      {items.map((p, i) => (
        <div key={p} className="flex items-center gap-2 rounded-md border bg-muted/50 py-1 pr-1 pl-3">
          <span className="flex-1 truncate font-mono text-xs">{p}</span>
          <Button size="icon" variant="ghost" className="size-6" aria-label={`Remove ${p}`} onClick={() => edit((c) => void c[list].splice(i, 1))}>
            <X />
          </Button>
        </div>
      ))}
      <Button size="sm" variant="outline" className="self-start" onClick={add}>
        {list === "roots" ? "Add root folder…" : "Add folder…"}
      </Button>
    </div>
  );
}

export function TargetsCard() {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  return (
    <Section title="Targets" description="Every folder directly inside a root is watched. New folders are picked up automatically. Symlinks work.">
      <Field label="Root folders">
        <PathList list="roots" />
      </Field>
      <Field label="Single folders">
        <PathList list="dirs" />
      </Field>
      <Field label="Exclude (folder name or full path, one per line)">
        <TextAreaField rows={3} value={config.exclude} format={(v) => v.join("\n")} parse={toLines} onChange={(v) => edit((c) => void (c.exclude = v))} />
      </Field>
      <Field
        label="Trust"
        hint={
          <>
            Sets <code>hasTrustDialogAccepted</code> for the roots and each folder in <code>~/.claude.json</code>.
          </>
        }
      >
        <CheckField checked={config.auto_trust} onChange={(v) => edit((c) => void (c.auto_trust = v))}>
          Trust every target folder in Claude Code (so the trust dialog never blocks a session)
        </CheckField>
      </Field>
    </Section>
  );
}
