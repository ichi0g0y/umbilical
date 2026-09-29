// The last lines of a folder's output. Stays at the bottom unless the user scrolls up.
import { useAtomValue } from "jotai";
import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { tabAtom } from "@/state/atoms";

export function LogView({ dirKey }: { dirKey: string }) {
  const tab = useAtomValue(tabAtom);
  const [text, setText] = useState("");
  const ref = useRef<HTMLPreElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    if (tab !== "status") return;
    let alive = true;
    const load = async () => {
      const lines = await api.tail(dirKey, 500).catch(() => null);
      if (alive && lines) setText(lines.join("\n"));
    };
    load();
    const timer = setInterval(load, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [dirKey, tab]);

  useEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [text]);

  return (
    <pre
      ref={ref}
      onScroll={(e) => {
        const el = e.currentTarget;
        stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      }}
      className="min-h-0 flex-1 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap break-all"
    >
      {text || "(no output yet)"}
    </pre>
  );
}
