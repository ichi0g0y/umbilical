// The settings of one folder. Empty fields mean "use the default".
import { useAtomValue } from "jotai";
import { CheckField, Field, NumberField, SelectField, TextAreaField, TextField } from "@/components/fields";
import { type DirOverride, PERMISSION_MODES, SPAWN_MODES } from "@/lib/api";
import { envText, parseEnv, toLines } from "@/lib/format";
import { useDraftConfig, useEditConfig } from "@/hooks/useConfig";
import { snapshotAtom } from "@/state/atoms";

/** Drop empty values, so the file keeps only real overrides. */
function clean(o: DirOverride): DirOverride | null {
  const out: DirOverride = {};
  if (o.enabled === false) out.enabled = false;
  if (o.name?.trim()) out.name = o.name.trim();
  if (o.permission_mode) out.permission_mode = o.permission_mode;
  if (o.spawn) out.spawn = o.spawn;
  if (o.capacity && o.capacity > 0) out.capacity = o.capacity;
  if (o.claude_bin?.trim()) out.claude_bin = o.claude_bin.trim();
  if (Array.isArray(o.extra_args)) out.extra_args = o.extra_args;
  if (o.env && Object.keys(o.env).length) out.env = o.env;
  return Object.keys(out).length ? out : null;
}

export function OverrideFields({ dirKey }: { dirKey: string }) {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  const snapshot = useAtomValue(snapshotAtom);
  const o = config.overrides[dirKey] ?? {};
  const dir = snapshot.dirs.find((d) => d.key === dirKey);

  const set = (change: (o: DirOverride) => void) =>
    edit((c) => {
      const next = { ...(c.overrides[dirKey] ?? {}) };
      change(next);
      const cleaned = clean(next);
      if (cleaned) c.overrides[dirKey] = cleaned;
      else delete c.overrides[dirKey];
    });

  return (
    <div className="flex flex-col gap-4">
      <CheckField checked={o.enabled !== false} onChange={(v) => set((x) => void (x.enabled = v ? null : false))}>
        Enabled
      </CheckField>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Session name">
          <TextField value={o.name ?? ""} placeholder={dir?.name ?? "folder name"} onChange={(v) => set((x) => void (x.name = v))} />
        </Field>
        <Field label="Permission mode">
          <SelectField
            value={o.permission_mode ?? null}
            options={PERMISSION_MODES}
            inherit={`default (${config.permission_mode})`}
            onChange={(v) => set((x) => void (x.permission_mode = v))}
          />
        </Field>
        <Field label="Spawn mode">
          <SelectField value={o.spawn ?? null} options={SPAWN_MODES} inherit={`default (${config.spawn})`} onChange={(v) => set((x) => void (x.spawn = v))} />
        </Field>
        <Field label="Capacity">
          <NumberField
            min={1}
            value={o.capacity ?? null}
            placeholder={config.capacity ? `default (${config.capacity})` : "default"}
            onChange={(v) => set((x) => void (x.capacity = v))}
          />
        </Field>
        <Field label="claude binary" className="col-span-2">
          <TextField mono value={o.claude_bin ?? ""} placeholder="default" onChange={(v) => set((x) => void (x.claude_bin = v))} />
        </Field>
      </div>
      <CheckField checked={Array.isArray(o.extra_args)} onChange={(v) => set((x) => void (x.extra_args = v ? (x.extra_args ?? []) : null))}>
        Use own extra arguments
      </CheckField>
      <TextAreaField
        value={o.extra_args ?? []}
        disabled={!Array.isArray(o.extra_args)}
        placeholder="one per line"
        format={(v) => v.join("\n")}
        parse={toLines}
        onChange={(v) => set((x) => void (x.extra_args = v))}
      />
      <Field label="Extra environment (KEY=VALUE, added to the defaults)">
        <TextAreaField value={o.env ?? {}} format={envText} parse={parseEnv} onChange={(v) => set((x) => void (x.env = v))} />
      </Field>
    </div>
  );
}
