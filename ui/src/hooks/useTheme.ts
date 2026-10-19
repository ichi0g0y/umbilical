// Applies the color theme and light / dark mode from the config.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAtomValue } from "jotai";
import { useEffect, useLayoutEffect } from "react";
import type { AppearanceConfig } from "@/lib/api";
import { APPEARANCE_MODES, THEMES } from "@/lib/themes";
import { draftConfigAtom, savedConfigAtom } from "@/state/atoms";

const DEFAULT: AppearanceConfig = { theme: "umbilical", mode: "system" };
const CACHE_KEY = "umbilical.appearance";

/** The last saved appearance. The config loads a moment after the window
 * opens, and this keeps the window from showing the default colors first.
 * Read once. A bad or old value falls back to the default, field by field. */
const CACHED: AppearanceConfig = (() => {
  try {
    const v = JSON.parse(localStorage.getItem(CACHE_KEY) ?? "null") as Partial<AppearanceConfig> | null;
    return {
      theme: THEMES.find((t) => t.value === v?.theme)?.value ?? DEFAULT.theme,
      mode: APPEARANCE_MODES.find((m) => m.value === v?.mode)?.value ?? DEFAULT.mode,
    };
  } catch {
    return DEFAULT;
  }
})();

/** Uses the draft config, so a change in Settings shows before it is saved. */
export function useTheme() {
  const draft = useAtomValue(draftConfigAtom)?.appearance;
  const saved = useAtomValue(savedConfigAtom)?.appearance;
  const { theme, mode } = draft ?? saved ?? CACHED;

  useEffect(() => {
    if (!saved) return;
    try {
      localStorage.setItem(CACHE_KEY, JSON.stringify(saved));
    } catch {
      // No storage: the window only shows the default colors for a moment.
    }
  }, [saved]);

  // A layout effect, so the first paint already has the right colors.
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => root.classList.toggle("dark", mode === "dark" || (mode === "system" && media.matches));
    apply();
    media.addEventListener("change", apply);
    // The title bar follows too. null means "follow the system".
    try {
      void getCurrentWindow()
        .setTheme(mode === "system" ? null : mode)
        .catch(() => {});
    } catch {
      // Not running inside Tauri (a plain browser).
    }
    return () => media.removeEventListener("change", apply);
  }, [theme, mode]);
}
