import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { LOCALES, matchLocale, t } from "../src/lib/i18n";

const dir = new URL("../src/locales/", import.meta.url);
type Catalog = Record<string, Record<string, string | Record<string, string>>>;
const read = (code: string): Catalog => JSON.parse(readFileSync(new URL(`${code}.json`, dir), "utf8"));
const en = read("en");
const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("i18n", () => {
  test("translates, interpolates, pluralises and falls back", () => {
    expect(t("common.cancel")).toBe("Cancel");
    expect(t("common.time.minutesAgo", { count: 5 })).toBe("5m ago");
    expect(t("missing.key")).toBe("missing.key");
  });

  test("browser languages map onto supported locales", () => {
    expect(matchLocale(["de-AT", "en"])).toBe("de");
    expect(matchLocale(["pt-BR"])).toBe("pt-PT");
    expect(matchLocale(["sr-Cyrl-RS"])).toBe("sr-Latn");
    expect(matchLocale(["uk-UA"])).toBe("uk");
    expect(matchLocale(["ja"])).toBe("en");
  });

  test("every listed locale has a catalog", () => {
    const files = readdirSync(dir).filter((f) => f.endsWith(".json")).map((f) => f.replace(/\.json$/, ""));
    for (const l of LOCALES) expect(files).toContain(l.code);
  });

  for (const { code } of LOCALES.filter((l) => l.code !== "en")) {
    test(`${code} mirrors en: keys, placeholders, plural forms`, () => {
      const cat = read(code);
      const categories = new Intl.PluralRules(code).resolvedOptions().pluralCategories;
      const problems: string[] = [];
      for (const [ns, entries] of Object.entries(en)) {
        for (const [key, value] of Object.entries(entries)) {
          const tr = cat[ns]?.[key];
          const id = `${ns}.${key}`;
          if (tr === undefined) {
            problems.push(`missing ${id}`);
            continue;
          }
          if (typeof value === "string") {
            if (typeof tr !== "string") problems.push(`${id}: expected a string`);
            else if (placeholders(tr).join() !== placeholders(value).join()) problems.push(`${id}: placeholders differ`);
          } else {
            if (typeof tr !== "object") {
              problems.push(`${id}: expected plural forms`);
              continue;
            }
            for (const c of categories) {
              if (c !== "zero" && c !== "two" && !(c in tr)) problems.push(`${id}: missing plural "${c}"`);
            }
            const want = placeholders(value.other).join();
            for (const [form, s] of Object.entries(tr)) {
              const got = placeholders(s).filter((p) => p !== "count").join();
              if (got !== want.split(",").filter((p) => p && p !== "count").join()) problems.push(`${id}.${form}: placeholders differ`);
            }
          }
        }
      }
      for (const [ns, entries] of Object.entries(cat)) {
        for (const key of Object.keys(entries)) if (en[ns]?.[key] === undefined) problems.push(`stale ${ns}.${key}`);
      }
      expect(problems).toEqual([]);
    });
  }
});
