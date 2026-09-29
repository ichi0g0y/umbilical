import { useAtom, useAtomValue } from "jotai";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Tabs, TabsContent } from "@/components/ui/tabs";
import { AboutView } from "@/features/about/AboutView";
import { Banners } from "@/features/Banners";
import { Header } from "@/features/Header";
import { LoginDialog } from "@/features/login/LoginDialog";
import { PermissionsView } from "@/features/permissions/PermissionsView";
import { usePermissions } from "@/features/permissions/usePermissions";
import { SettingsView } from "@/features/settings/SettingsView";
import { SetupView } from "@/features/setup/SetupView";
import { StatusView } from "@/features/status/StatusView";
import { useBackend, useSystemTheme } from "@/hooks/useBackend";
import { readyAtom, snapshotAtom, type Tab, tabAtom } from "@/state/atoms";

export function App() {
  useSystemTheme();
  useBackend();
  usePermissions();
  const ready = useAtomValue(readyAtom);
  const snapshot = useAtomValue(snapshotAtom);
  const [tab, setTab] = useAtom(tabAtom);

  if (!ready) return null;
  if (snapshot.needs_setup) return <SetupView />;

  return (
    <TooltipProvider delayDuration={400}>
      <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)} className="flex h-full flex-col gap-0">
        <Header />
        <Banners />
        <TabsContent value="status" className="min-h-0 flex-1">
          <StatusView />
        </TabsContent>
        <TabsContent value="settings" className="min-h-0 flex-1">
          <SettingsView />
        </TabsContent>
        <TabsContent value="permissions" className="min-h-0 flex-1 overflow-y-auto">
          <PermissionsView />
        </TabsContent>
        <TabsContent value="about" className="min-h-0 flex-1 overflow-y-auto">
          <AboutView />
        </TabsContent>
      </Tabs>
      <LoginDialog />
    </TooltipProvider>
  );
}
