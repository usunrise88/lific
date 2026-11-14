import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { LOCALES, matchLocale, t } from "../src/lib/i18n";
// @ts-expect-error plain .mjs shared with the build scripts
import { checkCatalog } from "../scripts/i18n-lib.mjs";

const dir = new URL("../src/locales/", import.meta.url);
type Catalog = Record<string, Record<string, string | Record<string, string>>>;
const read = (code: string): Catalog => JSON.parse(readFileSync(new URL(`${code}.json`, dir), "utf8"));
const en = read("en");

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
      expect(checkCatalog(code, en, read(code))).toEqual([]);
    });
  }
});
