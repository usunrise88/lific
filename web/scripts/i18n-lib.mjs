// Fork: shared checks for translation catalogs, used by the build scripts and
// by tests/i18n.test.ts so they cannot disagree.

export const placeholders = (s) => [...String(s).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

/** Plural categories a translation must provide for `code`. `zero`/`two` are
 *  rare and optional; `many` only matters for compact decimals ("1 million")
 *  in the Romance languages, so `other` covers it there. */
export function requiredPlurals(code) {
  const cats = new Intl.PluralRules(code).resolvedOptions().pluralCategories;
  const romance = /^(fr|es|pt|it|ca)(-|$)/.test(code);
  return cats.filter((c) => c !== "zero" && c !== "two" && !(c === "many" && romance));
}

/** Problems with `catalog` measured against `en`, as readable strings. */
export function checkCatalog(code, en, catalog) {
  const problems = [];
  const need = requiredPlurals(code);
  for (const [ns, entries] of Object.entries(en)) {
    for (const [key, value] of Object.entries(entries)) {
      const tr = catalog[ns]?.[key];
      const id = `${ns}.${key}`;
      if (tr === undefined) {
        problems.push(`missing ${id}`);
        continue;
      }
      if (typeof value === "string") {
        if (typeof tr !== "string") problems.push(`${id}: expected a string`);
        else if (placeholders(tr).join() !== placeholders(value).join()) problems.push(`${id}: placeholders differ (${placeholders(value)} vs ${placeholders(tr)})`);
        else if (tr.trim() === "") problems.push(`${id}: empty`);
        continue;
      }
      if (typeof tr !== "object" || tr === null) {
        problems.push(`${id}: expected plural forms`);
        continue;
      }
      for (const c of need) if (!(c in tr)) problems.push(`${id}: missing plural "${c}"`);
      if (!("other" in tr)) problems.push(`${id}: missing plural "other"`);
      const want = placeholders(value.other).filter((p) => p !== "count").join();
      for (const [form, s] of Object.entries(tr)) {
        if (placeholders(s).filter((p) => p !== "count").join() !== want) problems.push(`${id}.${form}: placeholders differ`);
      }
    }
  }
  for (const [ns, entries] of Object.entries(catalog)) {
    for (const key of Object.keys(entries)) if (en[ns]?.[key] === undefined) problems.push(`stale ${ns}.${key}`);
  }
  return problems;
}
