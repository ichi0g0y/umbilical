// Color themes. The colors are in src/index.css (Umbilical) and src/themes/.
// Keep the names in sync with THEMES in crates/umbilical-core/src/config/appearance.rs.
import type { ThemeName } from "@/lib/api";

export interface Theme {
  value: ThemeName;
  label: string;
  /** Light or dark. null follows the OS. */
  dark: boolean | null;
}

function versions(dark: boolean, list: [ThemeName, string][]): Theme[] {
  return list.map(([value, label]) => ({ value, label, dark }));
}

/** "system" first, then the light themes, then the dark ones. */
export const THEMES: Theme[] = [
  { value: "system", label: "System", dark: null },
  ...versions(false, [
    ["umbilical-light", "Umbilical Light"],
    ["catppuccin-latte", "Catppuccin Latte"],
    ["dracula-light", "Dracula Alucard"],
    ["nord-light", "Nord Light"],
    ["solarized-light", "Solarized Light"],
    ["gruvbox-light", "Gruvbox Light"],
    ["tokyo-night-light", "Tokyo Night Light"],
    ["rose-pine-light", "Rosé Pine Dawn"],
    ["one-light", "One Light"],
    ["github-light", "GitHub Light"],
  ]),
  ...versions(true, [
    ["umbilical-dark", "Umbilical Dark"],
    ["catppuccin-frappe", "Catppuccin Frappé"],
    ["catppuccin-macchiato", "Catppuccin Macchiato"],
    ["catppuccin-mocha", "Catppuccin Mocha"],
    ["dracula-dark", "Dracula"],
    ["nord-dark", "Nord"],
    ["solarized-dark", "Solarized Dark"],
    ["gruvbox-dark", "Gruvbox Dark"],
    ["tokyo-night-dark", "Tokyo Night"],
    ["rose-pine-dark", "Rosé Pine"],
    ["one-dark", "One Dark"],
    ["github-dark", "GitHub Dark"],
  ]),
];

export const DEFAULT_THEME = THEMES[0];

/** The theme with this name. An unknown or old name gives the default. */
export function findTheme(name: unknown): Theme {
  return THEMES.find((t) => t.value === name) ?? DEFAULT_THEME;
}
