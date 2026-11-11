import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { RADIX_THEMES } from "../src/lib/appearance/themes";

describe("radix themes", () => {
  const css = readFileSync(new URL("../src/radixThemes.css", import.meta.url), "utf8");
  const boot = readFileSync(new URL("../index.html", import.meta.url), "utf8");

  test("every theme has a light and a dark block", () => {
    for (const t of RADIX_THEMES) {
      expect(css).toContain(`html[data-theme-preset="${t.id}"] {`);
      expect(css).toContain(`html.dark[data-theme-preset="${t.id}"] {`);
    }
  });

  test("the pre-paint script in index.html knows every theme", () => {
    for (const t of RADIX_THEMES) expect(boot).toContain(`"${t.id}"`);
  });
});
