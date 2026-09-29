import type { DirStatus } from "./api";

/** "3m 20s" since a unix time in seconds. */
export function ago(unix: number): string {
  const s = Math.max(0, Math.floor(Date.now() / 1000 - unix));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
}

export function stateText(d: DirStatus): string {
  switch (d.state) {
    case "running": {
      const base = d.started_at ? `running for ${ago(d.started_at)}` : "running";
      return d.busy ? `${base} · ${d.busy} working` : base;
    }
    case "waiting": {
      if (!d.next_restart_at) return "waiting";
      const left = Math.max(0, d.next_restart_at - Math.floor(Date.now() / 1000));
      return `restart in ${left}s`;
    }
    case "stopped":
      return "stopped by you";
    case "disabled":
      return "disabled in settings";
  }
}

export type Tone = "ok" | "wait" | "bad" | "off";

/** Red for errors: something needs the user, or it crashed and waits to retry. */
export function dirTone(d: DirStatus): Tone {
  if (d.attention || (d.state === "waiting" && d.last_exit)) return "bad";
  if (d.state === "running") return "ok";
  if (d.state === "waiting") return "wait";
  return "off";
}

export function toLines(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
}

export function parseEnv(text: string): Record<string, string> {
  const env: Record<string, string> = {};
  for (const l of toLines(text)) {
    const i = l.indexOf("=");
    if (i > 0) env[l.slice(0, i).trim()] = l.slice(i + 1);
  }
  return env;
}

export function envText(env: Record<string, string> | undefined): string {
  return Object.entries(env ?? {})
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
}

/** A positive integer, or null for empty / invalid input. */
export function numOrNull(v: string): number | null {
  const n = parseInt(v, 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

export function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}
