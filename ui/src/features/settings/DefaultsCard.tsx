import { useAtomValue } from "jotai";
import { Field, NumberField, SelectField, TextAreaField, TextField } from "@/components/fields";
import { PERMISSION_MODES, SPAWN_MODES } from "@/lib/api";
import { envText, parseEnv, toLines } from "@/lib/format";
import { useDraftConfig, useEditConfig } from "@/hooks/useConfig";
import { snapshotAtom } from "@/state/atoms";
import { Section } from "./Section";

export function DefaultsCard() {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  const snapshot = useAtomValue(snapshotAtom);
  return (
    <Section id="defaults" title="Defaults for every folder">
      <div className="grid grid-cols-2 gap-4">
        <Field label="Permission mode">
          <SelectField value={config.permission_mode} options={PERMISSION_MODES} onChange={(v) => edit((c) => void (c.permission_mode = v!))} />
        </Field>
        <Field label="Spawn mode" hint="worktree needs git; other folders use same-dir.">
          <SelectField value={config.spawn} options={SPAWN_MODES} onChange={(v) => edit((c) => void (c.spawn = v!))} />
        </Field>
        <Field label="Capacity">
          <NumberField
            min={1}
            value={config.capacity}
            placeholder="claude default (32)"
            onChange={(v) => edit((c) => void (c.capacity = v && v > 0 ? v : null))}
          />
        </Field>
        <Field label="claude binary">
          <TextField
            mono
            value={config.claude_bin}
            placeholder={snapshot.claude_bin ? `auto: ${snapshot.claude_bin}` : "auto (not found)"}
            onChange={(v) => edit((c) => void (c.claude_bin = v.trim()))}
          />
        </Field>
      </div>
      <Field label="Extra arguments (one per line)">
        <TextAreaField value={config.extra_args} format={(v) => v.join("\n")} parse={toLines} onChange={(v) => edit((c) => void (c.extra_args = v))} />
      </Field>
      <Field label="Environment (KEY=VALUE, one per line)">
        <TextAreaField value={config.env} format={envText} parse={parseEnv} onChange={(v) => edit((c) => void (c.env = v))} />
      </Field>
    </Section>
  );
}
