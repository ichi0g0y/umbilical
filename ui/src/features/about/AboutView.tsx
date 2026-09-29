import { useAtomValue } from "jotai";
import type { ReactNode } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { api } from "@/lib/api";
import { appInfoAtom, snapshotAtom } from "@/state/atoms";
import { UpdateCard } from "./UpdateCard";

function Link({ path, children, reveal }: { path: string; children: ReactNode; reveal?: boolean }) {
  return (
    <button type="button" className="text-left break-all text-primary hover:underline" onClick={() => (reveal ? api.revealPath(path) : api.openPath(path))}>
      {children}
    </button>
  );
}

export function AboutView() {
  const info = useAtomValue(appInfoAtom);
  const snapshot = useAtomValue(snapshotAtom);
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-4">
      <Card className="gap-3 py-4">
        <CardHeader className="px-4">
          <CardTitle className="text-sm">
            Umbilical <span className="font-normal text-muted-foreground">{info && `v${info.version}`}</span>
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 px-4">
          <p>
            Keeps <code>claude remote-control</code> running in your project folders, so you can use them from claude.ai/code or the Claude app.
          </p>
          {info && (
            <dl className="grid grid-cols-[5rem_1fr] gap-x-3 gap-y-1">
              <dt className="text-muted-foreground">claude</dt>
              <dd className="font-mono text-xs break-all">{info.claude_bin ?? "not found"}</dd>
              <dt className="text-muted-foreground">Config</dt>
              <dd className="font-mono text-xs">
                <Link path={info.config_path} reveal>
                  {info.config_path}
                </Link>
              </dd>
              <dt className="text-muted-foreground">Logs</dt>
              <dd className="font-mono text-xs">
                <Link path={snapshot.log_dir}>{snapshot.log_dir}</Link>
              </dd>
              <dt className="text-muted-foreground">Source</dt>
              <dd>
                <Link path={info.repo}>{info.repo}</Link>
              </dd>
            </dl>
          )}
        </CardContent>
      </Card>
      <UpdateCard />
      <Card className="gap-2 border-wait/50 py-4">
        <CardHeader className="px-4">
          <CardTitle className="text-sm">Security</CardTitle>
        </CardHeader>
        <CardContent className="px-4">
          Sessions run with the permission mode you choose (default <code>bypassPermissions</code>). Anyone who can use your Claude account can run commands in
          these folders without asking. Turn on two-factor authentication.
        </CardContent>
      </Card>
    </div>
  );
}
