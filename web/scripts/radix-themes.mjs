// Fork: generates the Radix Colors themes.
//
//   node scripts/radix-themes.mjs
//
// Writes src/radixThemes.css (one light and one dark block per theme, keyed
// on `data-theme-preset` on <html>) and src/lib/appearance/themes.ts (the
// picker's names and swatches). Both are committed, so building the app does
// not run this. Re-run after changing THEMES or upgrading @radix-ui/colors.
//
// Each theme pairs a Radix gray scale with an accent scale and maps their
// 12 steps onto the app's color tokens (see `tokens`). Text pairs are
// checked against WCAG AA (4.5:1) in both modes; the accent is darkened
// (light) or lightened (dark) toward step 12 until it passes, and the script
// fails if anything still does not.

import { writeFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const radix = require("@radix-ui/colors");

/** [id, label, gray, accent]. Radix's recommended natural pairings. */
const THEMES = [
  ["indigo", "Indigo", "slate", "indigo"],
  ["ocean", "Ocean", "slate", "blue"],
  ["lagoon", "Lagoon", "slate", "cyan"],
  ["iris", "Iris", "mauve", "violet"],
  ["berry", "Berry", "mauve", "crimson"],
  ["plum", "Plum", "mauve", "plum"],
  ["jade", "Jade", "sage", "jade"],
  ["teal", "Teal", "sage", "teal"],
  ["meadow", "Meadow", "olive", "grass"],
  ["desert", "Desert", "sand", "amber"],
  ["ember", "Ember", "sand", "orange"],
  ["graphite", "Graphite", "gray", "gray"],
];

function scale(name, dark) {
  const s = radix[name + (dark ? "Dark" : "")];
  if (!s) throw new Error(`no Radix scale ${name}`);
  return (step) => s[`${name}${step}`];
}

// ── color math ──────────────────────────────────────────

const hex2rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
const rgb2hex = (c) => "#" + c.map((v) => Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16).padStart(2, "0")).join("");
const lin = (v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const lum = (h) => {
  const [r, g, b] = hex2rgb(h).map(lin);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
export const contrast = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};
/** Linear sRGB mix, `t` of `b` into `a`. */
const mix = (a, b, t) => {
  const [p, q] = [hex2rgb(a), hex2rgb(b)];
  return rgb2hex(p.map((v, i) => v + (q[i] - v) * t));
};

/** Move `color` toward `toward` in small steps until every check passes. */
function settle(color, toward, checks) {
  for (let t = 0; t <= 1.0001; t += 0.05) {
    const c = mix(color, toward, t);
    if (checks.every((f) => f(c))) return c;
  }
  return null;
}

// ── token mapping ───────────────────────────────────────

function tokens(gray, accent, dark) {
  const g = scale(gray, dark);
  const a = scale(accent, dark);
  const red = scale("red", dark);
  const grass = scale("grass", dark);
  const orange = scale("orange", dark);
  const t = {};

  // Surfaces: recessed content floor, raised cards, the chrome shell.
  if (dark) {
    t.bg = g(1);
    t.chrome = g(2);
    t.surface = g(3);
    t["bg-subtle"] = g(2);
  } else {
    t.bg = g(3);
    t.chrome = g(1);
    t.surface = g(1);
    t["bg-subtle"] = g(4);
  }
  t.border = g(6);
  t.text = g(12);
  t["text-faint"] = g(11);
  t["text-muted"] = mix(g(11), g(12), 0.3);

  // Accent: step 11 is Radix's accessible accent text; filled controls use
  // the same token, so it must also carry --accent-text. Monochrome uses 12.
  const accentText = dark ? g(1) : "#ffffff";
  const base = accent === "gray" ? a(12) : a(11);
  const subtle = a(3);
  const ok = (c) =>
    contrast(c, accentText) >= 4.5 &&
    contrast(c, t.bg) >= 4.5 &&
    contrast(c, t.surface) >= 4.5 &&
    contrast(c, subtle) >= 4.5;
  t.accent = settle(base, a(12), [ok]);
  if (!t.accent) throw new Error(`${gray}/${accent} ${dark ? "dark" : "light"}: no accessible accent`);
  t["accent-hover"] = mix(t.accent, a(12), 0.35);
  t["accent-text"] = accentText;
  t["accent-subtle"] = subtle;

  t.error = red(11);
  t["error-bg"] = red(3);
  t["error-text"] = dark ? g(1) : "#ffffff";
  t.success = grass(11);
  t["success-bg"] = grass(3);
  t["btn-success"] = grass(9);
  t["btn-success-hover"] = grass(10);
  t["btn-success-text"] = "#0e1511";
  t.warn = orange(9);
  // Amber text sits on a 15% warn tint (the active "Closed" toggles etc.).
  const warnFill = mix(t.bg, t.warn, 0.15);
  t["warn-text"] = settle(orange(11), orange(12), [
    (c) => contrast(c, warnFill) >= 4.5 && contrast(c, t.surface) >= 4.5,
  ]);

  t["panel-bg"] = dark ? g(3) : g(12);
  t["panel-text"] = dark ? g(12) : g(1);
  t["panel-muted"] = dark ? g(10) : g(8);
  t["shadow-recess"] = dark ? "rgba(0, 0, 0, 0.22)" : "rgba(0, 0, 0, 0.06)";
  return t;
}

/** Text pairs that must clear AA in every theme and mode. */
const CHECKS = [
  ["text", "bg"], ["text", "surface"], ["text-muted", "surface"], ["text-muted", "bg"],
  ["text-faint", "surface"], ["text-faint", "bg"], ["text-faint", "chrome"],
  ["accent", "bg"], ["accent", "surface"], ["accent-text", "accent"], ["accent", "accent-subtle"],
  ["error", "surface"], ["success", "surface"], ["btn-success-text", "btn-success"],
  ["panel-text", "panel-bg"], ["warn-text", "surface"],
];

// ── output ──────────────────────────────────────────────

let css = `/* Generated by scripts/radix-themes.mjs from @radix-ui/colors; do not edit.
 * Themes are selected with data-theme-preset on <html> (see lib/theme.ts);
 * "classic" is the original palette in app.css and has no block here.
 * html[...] outranks the [data-accent] presets, which a theme replaces. */\n`;
const meta = [];
const report = [];

for (const [id, label, gray, accent] of THEMES) {
  const light = tokens(gray, accent, false);
  const dark = tokens(gray, accent, true);
  for (const [mode, t] of [["light", light], ["dark", dark]]) {
    for (const [fg, bg] of CHECKS) {
      const r = contrast(t[fg], t[bg]);
      if (r < 4.5) report.push(`${id} ${mode}: ${fg} on ${bg} ${r.toFixed(2)}`);
    }
  }
  const block = (sel, t) =>
    `${sel} {\n${Object.entries(t).map(([k, v]) => `  --${k}: ${v};`).join("\n")}\n}\n`;
  css += `\n/* ${label}: ${gray} + ${accent} */\n`;
  css += block(`html[data-theme-preset="${id}"]`, light);
  css += block(`html.dark[data-theme-preset="${id}"]`, dark);
  meta.push({
    id,
    label,
    gray,
    accent,
    light: { bg: light.bg, surface: light.surface, accent: light.accent, text: light.text },
    dark: { bg: dark.bg, surface: dark.surface, accent: dark.accent, text: dark.text },
  });
}

if (report.length) {
  console.error("Contrast failures:\n" + report.join("\n"));
  process.exit(1);
}

writeFileSync(new URL("../src/radixThemes.css", import.meta.url), css);
writeFileSync(
  new URL("../src/lib/appearance/themes.ts", import.meta.url),
  `// Generated by scripts/radix-themes.mjs; do not edit.
// Picker metadata for the Radix Colors themes in src/radixThemes.css.

export interface ThemeSwatch {
  bg: string;
  surface: string;
  accent: string;
  text: string;
}

export interface RadixTheme {
  id: string;
  label: string;
  gray: string;
  accent: string;
  light: ThemeSwatch;
  dark: ThemeSwatch;
}

export const RADIX_THEMES: RadixTheme[] = ${JSON.stringify(meta, null, 2)};
`,
);
console.log(`wrote ${THEMES.length} themes; every text pair clears 4.5:1 in light and dark`);
