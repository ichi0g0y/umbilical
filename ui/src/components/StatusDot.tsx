import type { Tone } from "@/lib/format";
import { cn } from "@/lib/utils";

const TONE: Record<Tone, string> = {
  ok: "bg-ok",
  wait: "bg-wait",
  bad: "bg-bad",
  off: "bg-off",
};

export function StatusDot({ tone, className }: { tone: Tone; className?: string }) {
  return <span aria-hidden className={cn("inline-block size-2.5 shrink-0 rounded-full", TONE[tone], className)} />;
}
