// Color themes. The colors are in src/index.css (umbilical) and src/themes/.
// Keep the names in sync with THEMES in crates/umbilical-core/src/config/appearance.rs.
import type { AppearanceMode, ThemeName } from "@/lib/api";

export const THEMES: { value: ThemeName; label: string }[] = [
  { value: "umbilical", label: "Umbilical (purple)" },
  { value: "catppuccin", label: "Catppuccin" },
  { value: "dracula", label: "Dracula" },
  { value: "nord", label: "Nord" },
  { value: "solarized", label: "Solarized" },
  { value: "gruvbox", label: "Gruvbox" },
  { value: "tokyo-night", label: "Tokyo Night" },
  { value: "rose-pine", label: "Rosé Pine" },
  { value: "one", label: "One" },
  { value: "github", label: "GitHub" },
];

export const APPEARANCE_MODES: { value: AppearanceMode; label: string }[] = [
  { value: "system", label: "Same as the system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];
