import { useAtomValue } from "jotai";
import { CheckField, Field, NumberField, SelectField, TextField } from "@/components/fields";
import type { ShowIn } from "@/lib/api";
import { useDraftConfig, useEditConfig } from "@/hooks/useConfig";
import { snapshotAtom } from "@/state/atoms";
import { Section } from "./Section";

export function RestartCard() {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  return (
    <Section title="Restart and scan">
      <div className="grid grid-cols-2 gap-4">
        <Field label="First retry after (s)">
          <NumberField min={1} value={config.backoff_min_secs} onChange={(v) => edit((c) => void (c.backoff_min_secs = v || 5))} />
        </Field>
        <Field label="Longest wait (s)">
          <NumberField min={1} value={config.backoff_max_secs} onChange={(v) => edit((c) => void (c.backoff_max_secs = v || 300))} />
        </Field>
        <Field label="Reset wait after running (s)">
          <NumberField min={0} value={config.stable_reset_secs} onChange={(v) => edit((c) => void (c.stable_reset_secs = v ?? 0))} />
        </Field>
        <Field label="Scan every (s)">
          <NumberField min={1} value={config.scan_interval_secs} onChange={(v) => edit((c) => void (c.scan_interval_secs = v || 5))} />
        </Field>
      </div>
    </Section>
  );
}

const SHOW_IN: { value: ShowIn; label: string }[] = [
  { value: "menu_bar", label: "Menu bar only" },
  { value: "menu_bar_and_dock", label: "Menu bar and Dock" },
  { value: "dock", label: "Dock only" },
];

const MB = 1024 * 1024;

export function AppCard() {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  const snapshot = useAtomValue(snapshotAtom);
  return (
    <Section title="App">
      <div className="grid grid-cols-2 gap-4">
        <Field label="Show in" hint="On Windows the Dock is the taskbar.">
          <SelectField value={config.show_in} options={SHOW_IN} onChange={(v) => edit((c) => void (c.show_in = v as ShowIn))} />
        </Field>
        <Field label="Start">
          <CheckField checked={config.autostart} onChange={(v) => edit((c) => void (c.autostart = v))}>
            Start at login
          </CheckField>
        </Field>
        <Field label="Log folder">
          <TextField
            mono
            value={config.log_dir}
            placeholder={snapshot.log_dir ? `default: ${snapshot.log_dir}` : "default"}
            onChange={(v) => edit((c) => void (c.log_dir = v.trim()))}
          />
        </Field>
        <Field label="Max log size (MB)">
          <NumberField min={0} value={Math.round(config.log_max_bytes / MB)} onChange={(v) => edit((c) => void (c.log_max_bytes = Math.max(0, v ?? 0) * MB))} />
        </Field>
      </div>
      <Field
        label="Debug"
        hint={
          <>
            Writes more detail to <code>umbilical.log</code>, and claude's own debug log to <code>sessions/&lt;folder&gt;.claude-debug.log</code>. Changing it
            restarts the sessions.
          </>
        }
      >
        <CheckField checked={config.debug} onChange={(v) => edit((c) => void (c.debug = v))}>
          Debug logging
        </CheckField>
      </Field>
    </Section>
  );
}

export function UpdatesCard() {
  const config = useDraftConfig()!;
  const edit = useEditConfig();
  const u = config.update;
  return (
    <Section title="Updates" description="Updates wait until no session is working, because the restart stops running turns.">
      <div className="grid grid-cols-2 gap-4">
        <Field label="Channel">
          <SelectField value={u.channel} options={["stable", "nightly"]} onChange={(v) => edit((c) => void (c.update.channel = v as "stable" | "nightly"))} />
        </Field>
        <Field label="Check every (hours, 0 = never)">
          <NumberField min={0} value={u.check_interval_hours} onChange={(v) => edit((c) => void (c.update.check_interval_hours = v ?? 0))} />
        </Field>
      </div>
      <CheckField checked={u.check_on_start} onChange={(v) => edit((c) => void (c.update.check_on_start = v))}>
        Check at start
      </CheckField>
      <CheckField checked={u.auto_install} onChange={(v) => edit((c) => void (c.update.auto_install = v))}>
        Install without asking
      </CheckField>
    </Section>
  );
}
