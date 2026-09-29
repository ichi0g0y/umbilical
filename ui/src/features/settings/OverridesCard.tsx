import { useAtom, useAtomValue } from "jotai";
import { useEffect, useRef } from "react";
import { CheckField, Field, NumberField, SelectField, TextAreaField, TextField } from "@/components/fields";
import { type DirOverride, PERMISSION_MODES, SPAWN_MODES } from "@/lib/api";
import { envText, parseEnv, toLines } from "@/lib/format";
import { useDraftConfig, useEditConfig } from "@/hooks/useConfig";
import { overrideKeyAtom, snapshotAtom } from "@/state/atoms";
import { Section } from "./Section";

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

export function OverridesCard() {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  const snapshot = useAtomValue(snapshotAtom);
  const [key, setKey] = useAtom(overrideKeyAtom);
  const ref = useRef<HTMLDivElement>(null);

  // Opened from "Folder settings" in the status view: scroll here.
  useEffect(() => {
    if (key) ref.current?.scrollIntoView({ block: "start", behavior: "smooth" });
  }, [key]);

  const keys = [...new Set([...snapshot.dirs.map((d) => d.key), ...Object.keys(config.overrides)])].sort();
  const options = keys.map((k) => {
    const d = snapshot.dirs.find((x) => x.key === k);
    return { value: k, label: d ? `${d.name} — ${k}` : `${k} (not found now)` };
  });
  const o = (key && config.overrides[key]) || {};
  const dir = snapshot.dirs.find((d) => d.key === key);
  const set = (change: (o: DirOverride) => void) =>
    edit((c) => {
      if (!key) return;
      const next = { ...(c.overrides[key] ?? {}) };
      change(next);
      const cleaned = clean(next);
      if (cleaned) c.overrides[key] = cleaned;
      else delete c.overrides[key];
    });

  return (
    <div ref={ref} id="folders" className="scroll-mt-4">
      <Section title="Per-folder settings" description="Empty fields use the defaults above.">
        <Field label="Folder">
          <SelectField value={key} options={options} inherit="Choose a folder…" onChange={setKey} />
        </Field>
        {key && (
          <div key={key} className="flex flex-col gap-4">
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
                <SelectField
                  value={o.spawn ?? null}
                  options={SPAWN_MODES}
                  inherit={`default (${config.spawn})`}
                  onChange={(v) => set((x) => void (x.spawn = v))}
                />
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
        )}
      </Section>
    </div>
  );
}
