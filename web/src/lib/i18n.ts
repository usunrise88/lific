// Fork: interface translations.
//
// Strings live in src/locales/<code>.json, one object per namespace:
//
//   { "feed": { "empty.title": "No activity yet",
//               "rows": { "one": "{count} row", "other": "{count} rows" } } }
//
// and are read with `t("feed.empty.title")` or `t("feed.rows", { count })`.
// English is bundled and is the fallback for any missing key; the other
// locales load on demand. Plain TypeScript (no runes) so pure modules and
// their tests can call `t`; App.svelte re-mounts the interface when the
// locale changes (see `onLocaleChange`). Conventions: src/locales/README.md.

import en from "../locales/en.json";

export interface LocaleInfo {
  code: string;
  /** The language's own name, as the picker shows it. */
  name: string;
}

export const LOCALES: LocaleInfo[] = [
  { code: "en", name: "English" },
  { code: "fr", name: "Français" },
  { code: "de", name: "Deutsch" },
  { code: "es", name: "Español" },
  { code: "pt-PT", name: "Português (Portugal)" },
  { code: "nl", name: "Nederlands" },
  { code: "hu", name: "Magyar" },
  { code: "ro", name: "Română" },
  { code: "bg", name: "Български" },
  { code: "hr", name: "Hrvatski" },
  { code: "sr-Latn", name: "Srpski (latinica)" },
  { code: "ru", name: "Русский" },
  { code: "uk", name: "Українська" },
];

type PluralForms = Partial<Record<Intl.LDMLPluralRule, string>> & { other: string };
type Entry = string | PluralForms;
type Catalog = Record<string, Record<string, Entry>>;

const STORAGE_KEY = "lific_locale";
const CODES = new Set(LOCALES.map((l) => l.code));

const state = {
  locale: "en",
  catalogs: { en: en as Catalog } as Record<string, Catalog>,
};

const listeners = new Set<(code: string) => void>();

/** Run `fn` after every locale switch; returns the unsubscribe. */
export function onLocaleChange(fn: (code: string) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** The active locale code (BCP 47), for Intl formatters. */
export function locale(): string {
  return state.locale;
}

function lookup(catalog: Catalog | undefined, key: string): Entry | undefined {
  if (!catalog) return undefined;
  const dot = key.indexOf(".");
  if (dot < 0) return undefined;
  return catalog[key.slice(0, dot)]?.[key.slice(dot + 1)];
}

const pluralRules = new Map<string, Intl.PluralRules>();
function pluralOf(code: string, n: number): Intl.LDMLPluralRule {
  let rules = pluralRules.get(code);
  if (!rules) {
    rules = new Intl.PluralRules(code);
    pluralRules.set(code, rules);
  }
  return rules.select(n);
}

/** Translate `key`, filling `{name}` placeholders from `params`. An entry
 *  with plural forms picks one by `params.count`. Missing keys fall back to
 *  English, then to the key itself. */
export function t(key: string, params?: Record<string, string | number>): string {
  const code = state.locale;
  let entry = lookup(state.catalogs[code], key);
  let from = code;
  if (entry === undefined) {
    entry = lookup(state.catalogs.en, key);
    from = "en";
  }
  if (entry === undefined) return key;
  let text: string;
  if (typeof entry === "string") {
    text = entry;
  } else {
    const n = Number(params?.count ?? 0);
    text = entry[pluralOf(from, n)] ?? entry.other;
  }
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) =>
    name in params ? String(name === "count" ? formatNumber(Number(params[name])) : params[name]) : m,
  );
}

/** A number in the active locale's notation. */
export function formatNumber(n: number): string {
  return Number.isFinite(n) ? n.toLocaleString(state.locale) : String(n);
}

async function load(code: string): Promise<Catalog | null> {
  if (code === "en") return en as Catalog;
  try {
    // One JSON per locale, split into its own chunk by the bundler.
    const mod = await import(`../locales/${code}.json`);
    return (mod.default ?? mod) as Catalog;
  } catch (err) {
    console.warn(`[lific] could not load locale ${code}`, err);
    return null;
  }
}

/** Switch the interface language and remember it in this browser. */
export async function setLocale(code: string): Promise<void> {
  if (!CODES.has(code)) code = "en";
  if (!state.catalogs[code]) {
    const catalog = await load(code);
    if (!catalog) return;
    state.catalogs = { ...state.catalogs, [code]: catalog };
  }
  state.locale = code;
  for (const fn of listeners) fn(code);
  try {
    localStorage.setItem(STORAGE_KEY, code);
  } catch {
    // private mode: the choice lasts for this page only
  }
  if (typeof document !== "undefined") document.documentElement.lang = code;
}

/** The best supported locale for a list of browser languages. */
export function matchLocale(preferred: readonly string[]): string {
  for (const raw of preferred) {
    const tag = raw.toLowerCase();
    const exact = LOCALES.find((l) => l.code.toLowerCase() === tag);
    if (exact) return exact.code;
    const base = tag.split("-")[0];
    // Serbian in Cyrillic still gets the Latin catalog; Portuguese of any
    // region gets the European one.
    if (base === "sr") return "sr-Latn";
    if (base === "pt") return "pt-PT";
    const byBase = LOCALES.find((l) => l.code.toLowerCase() === base);
    if (byBase) return byBase.code;
  }
  return "en";
}

/** Pick the stored locale, else the browser's, and load it. */
export async function initLocale(): Promise<void> {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(STORAGE_KEY);
  } catch {
    // ignore
  }
  const code =
    stored && CODES.has(stored)
      ? stored
      : matchLocale(typeof navigator === "undefined" ? [] : navigator.languages ?? [navigator.language]);
  await setLocale(code);
}
