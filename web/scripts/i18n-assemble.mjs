// Fork: assemble a locale from per-namespace work files and check it.
//
//   node scripts/i18n-assemble.mjs <code>          # check and, if clean, write
//   node scripts/i18n-assemble.mjs <code> --check  # check only
//
// Reads src/locales/_tr/<code>/<namespace>.json (each `{ "key": "…" }`, the
// same keys as that namespace in en.json), writes src/locales/<code>.json in
// en.json's order, and fails on anything checkCatalog() reports. Namespaces
// that have no work file yet are reported as missing keys.

import { existsSync, readFileSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import { checkCatalog } from "./i18n-lib.mjs";

const [, , code, flag] = process.argv;
if (!code) {
  console.error("usage: node scripts/i18n-assemble.mjs <code> [--check]");
  process.exit(2);
}

const dir = new URL("../src/locales/", import.meta.url);
const en = JSON.parse(readFileSync(new URL("en.json", dir), "utf8"));
const work = new URL(`_tr/${code}/`, dir);
const files = existsSync(work) ? readdirSync(work).filter((f) => f.endsWith(".json")) : [];

const problems = [];
const catalog = {};
for (const ns of Object.keys(en)) {
  const f = `${ns}.json`;
  if (!files.includes(f)) continue;
  try {
    catalog[ns] = JSON.parse(readFileSync(new URL(f, work), "utf8"));
  } catch (err) {
    problems.push(`${f}: not valid JSON (${err.message})`);
  }
}
for (const f of files) if (en[f.replace(/\.json$/, "")] === undefined) problems.push(`${f}: no such namespace in en.json`);

problems.push(...checkCatalog(code, en, catalog));

const keys = Object.values(en).reduce((n, e) => n + Object.keys(e).length, 0);
if (problems.length) {
  console.error(`${code}: ${problems.length} problems (of ${keys} keys)\n` + problems.slice(0, 60).join("\n") + (problems.length > 60 ? `\n… and ${problems.length - 60} more` : ""));
  process.exit(1);
}
if (flag !== "--check") {
  const out = {};
  for (const ns of Object.keys(en)) {
    out[ns] = {};
    for (const key of Object.keys(en[ns])) out[ns][key] = catalog[ns][key];
  }
  const tmp = new URL(`${code}.json.${process.pid}.tmp`, dir);
  writeFileSync(tmp, JSON.stringify(out, null, 2) + "\n");
  renameSync(tmp, new URL(`${code}.json`, dir));
}
console.log(`${code}: ${keys} keys OK${flag === "--check" ? " (checked only)" : ", written"}`);
