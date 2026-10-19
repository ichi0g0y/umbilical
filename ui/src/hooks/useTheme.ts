// Applies the color theme from the config. A theme is light or dark by itself;
// only "system" follows the OS.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAtomValue } from "jotai";
import { useEffect, useLayoutEffect } from "react";
import type { ThemeName } from "@/lib/api";
import { findTheme } from "@/lib/themes";
import { draftConfigAtom, savedConfigAtom } from "@/state/atoms";

const CACHE_KEY = "umbilical.appearance";

/** The last saved theme. The config loads a moment after the window opens,
 * and this keeps the window from showing the default colors first. Read once.
 * A bad or old value (like { theme: "catppuccin", mode: "dark" }) gives the
 * default; the config replaces it a moment later. */
const CACHED: ThemeName = (() => {
  try {
    const v = JSON.parse(localStorage.getItem(CACHE_KEY) ?? "null") as { theme?: unknown } | null;
    return findTheme(v?.theme).value;
  } catch {
    return findTheme(null).value;
  }
})();

/** Uses the draft config, so a change in Settings shows before it is saved. */
export function useTheme() {
  const draft = useAtomValue(draftConfigAtom)?.appearance;
  const saved = useAtomValue(savedConfigAtom)?.appearance;
  const theme = findTheme((draft ?? saved)?.theme ?? CACHED);

  useEffect(() => {
    if (!saved) return;
    try {
      localStorage.setItem(CACHE_KEY, JSON.stringify({ theme: saved.theme }));
    } catch {
      // No storage: the window only shows the default colors for a moment.
    }
  }, [saved]);

  // A layout effect, so the first paint already has the right colors.
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme.value;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => root.classList.toggle("dark", theme.dark ?? media.matches);
    apply();
    // Only "system" changes with the OS.
    if (theme.dark === null) media.addEventListener("change", apply);
    // The title bar follows too. null means "follow the system".
    try {
      void getCurrentWindow()
        .setTheme(theme.dark === null ? null : theme.dark ? "dark" : "light")
        .catch(() => {});
    } catch {
      // Not running inside Tauri (a plain browser).
    }
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}
