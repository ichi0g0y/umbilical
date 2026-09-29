// Form fields for the settings screen. Text fields keep their own text while
// the user types, and give the parsed value back to the config.
import { useAtomValue } from "jotai";
import { type ReactNode, useEffect, useId, useState } from "react";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { configRevisionAtom } from "@/state/atoms";

export function Field({ label, hint, children, className }: { label: ReactNode; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <Label className="text-xs font-medium text-muted-foreground">{label}</Label>
      {children}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
}

/** Local text that resets when the config is loaded again. */
function useLocalText(initial: string) {
  const revision = useAtomValue(configRevisionAtom);
  const [text, setText] = useState(initial);
  // Reset only on a new revision, not on every value change.
  useEffect(() => setText(initial), [revision]);
  return [text, setText] as const;
}

export function TextField(props: { value: string; onChange: (v: string) => void; placeholder?: string; mono?: boolean }) {
  return (
    <Input
      value={props.value}
      placeholder={props.placeholder}
      spellCheck={false}
      className={cn(props.mono && "font-mono text-xs")}
      onChange={(e) => props.onChange(e.target.value)}
    />
  );
}

/** A number field. `null` means empty (use the default). */
export function NumberField(props: { value: number | null; onChange: (v: number | null) => void; placeholder?: string; min?: number }) {
  const [text, setText] = useLocalText(props.value == null ? "" : String(props.value));
  return (
    <Input
      type="number"
      min={props.min}
      value={text}
      placeholder={props.placeholder}
      onChange={(e) => {
        setText(e.target.value);
        const n = parseInt(e.target.value, 10);
        props.onChange(Number.isFinite(n) ? n : null);
      }}
    />
  );
}

/** Multi-line text. `parse` turns the text into the config value. */
export function TextAreaField<T>(props: {
  value: T;
  format: (v: T) => string;
  parse: (text: string) => T;
  onChange: (v: T) => void;
  placeholder?: string;
  rows?: number;
  disabled?: boolean;
}) {
  const [text, setText] = useLocalText(props.format(props.value));
  return (
    <Textarea
      rows={props.rows ?? 2}
      value={text}
      disabled={props.disabled}
      placeholder={props.placeholder}
      spellCheck={false}
      className="min-h-0 font-mono text-xs"
      onChange={(e) => {
        setText(e.target.value);
        props.onChange(props.parse(e.target.value));
      }}
    />
  );
}

const INHERIT = "__default__";

/** A select. With `inherit`, an extra first item means "use the default" (value null). */
export function SelectField(props: {
  value: string | null;
  options: { value: string; label: string }[] | string[];
  onChange: (v: string | null) => void;
  inherit?: string;
}) {
  const options = props.options.map((o) => (typeof o === "string" ? { value: o, label: o } : o));
  return (
    <Select value={props.value ?? INHERIT} onValueChange={(v) => props.onChange(v === INHERIT ? null : v)}>
      <SelectTrigger className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {props.inherit && <SelectItem value={INHERIT}>{props.inherit}</SelectItem>}
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export function CheckField({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode }) {
  const id = useId();
  return (
    <div className="flex items-center gap-2">
      <Checkbox id={id} checked={checked} onCheckedChange={(v) => onChange(v === true)} />
      <Label htmlFor={id} className="font-normal">
        {children}
      </Label>
    </div>
  );
}
