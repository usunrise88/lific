import type { AccentPreset } from "../theme";
import { t } from "../i18n";

/**
 * Swatch metadata for the accent picker (LIF-238). The swatch color is
 * the dark-mode --accent value for each preset — it's the more
 * saturated/vivid of the two and reads clearly as a small dot on
 * either a light or dark Settings page.
 *
 * Keep in sync with the `[data-accent="..."]` / `.dark[data-accent="..."]`
 * blocks in app.css — this is presentation-only metadata, not the
 * source of truth for the actual CSS variable values.
 */
const PRESETS: { id: AccentPreset; swatch: string }[] = [
  { id: "indigo", swatch: "#9287d7" },
  { id: "teal", swatch: "#4dd9c7" },
  { id: "rose", swatch: "#f27a9c" },
  { id: "amber", swatch: "#e0a530" },
  { id: "green", swatch: "#5cd192" },
  { id: "violet", swatch: "#b48af0" },
];

/** The accent's display name in the interface language. */
export function accentLabel(id: AccentPreset): string {
  return t(`appearance.accent.${id}`);
}

/** The presets with their labels, built at call time (fork: i18n). */
export function accentPresets(): { id: AccentPreset; label: string; swatch: string }[] {
  return PRESETS.map((p) => ({ ...p, label: accentLabel(p.id) }));
}
