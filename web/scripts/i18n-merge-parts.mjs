// Fork, temporary: merge the English string parts written per area during
// the extraction pass (src/locales/_parts/*.json) into src/locales/en.json.
//
//   node scripts/i18n-merge-parts.mjs
//
// Keys already in en.json are kept; a part may add keys but must not give
// an existing key a different value (that is reported and fails). Safe to
// run concurrently: the result is written to a temp file and renamed.

import { readFileSync, readdirSync, renameSync, writeFileSync, existsSync } from "node:fs";

const dir = new URL("../src/locales/", import.meta.url);
const partsDir = new URL("_parts/", dir);
const enPath = new URL("en.json", dir);

const base = JSON.parse(readFileSync(enPath, "utf8"));
const conflicts = [];
const parts = existsSync(partsDir) ? readdirSync(partsDir).filter((f) => f.endsWith(".json")).sort() : [];

for (const file of parts) {
  let part;
  try {
    part = JSON.parse(readFileSync(new URL(file, partsDir), "utf8"));
  } catch (err) {
    conflicts.push(`${file}: not valid JSON (${err.message})`);
    continue;
  }
  for (const [ns, entries] of Object.entries(part)) {
    base[ns] ??= {};
    for (const [key, value] of Object.entries(entries)) {
      const have = base[ns][key];
      if (have !== undefined && JSON.stringify(have) !== JSON.stringify(value)) {
        conflicts.push(`${file}: ${ns}.${key} differs from an existing value`);
        continue;
      }
      base[ns][key] = value;
    }
  }
}

const tmp = new URL(`en.json.${process.pid}.tmp`, dir);
writeFileSync(tmp, JSON.stringify(base, null, 2) + "\n");
renameSync(tmp, enPath);

const keys = Object.values(base).reduce((n, entries) => n + Object.keys(entries).length, 0);
console.log(`en.json: ${Object.keys(base).length} namespaces, ${keys} keys from ${parts.length} parts`);
if (conflicts.length) {
  console.error("conflicts:\n" + conflicts.join("\n"));
  process.exit(1);
}
