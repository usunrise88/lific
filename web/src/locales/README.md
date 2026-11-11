# Interface translations (fork)

`en.json` is the source; every other `<code>.json` mirrors its keys. Read
strings with `t()` from `src/lib/i18n.svelte.ts`.

## Shape

```json
{
  "feed": {
    "empty.title": "No activity yet",
    "rows": { "one": "{count} row", "other": "{count} rows" },
    "posted": "Comment posted on {identifier}"
  }
}
```

- The top level is a **namespace**, one per area (`feed`, `forMe`, `issueList`,
  `settings`…). Keys inside are dotted, short and describe the place, not the
  text: `filters.reset`, `empty.title`, `toast.saved`.
- Shared words used across the app live in `common`: `common.cancel`,
  `common.save`, `common.delete`, `common.loading`… Reuse them instead of
  adding a copy.
- Placeholders are `{name}`. Never build a sentence by concatenating
  translated fragments: the word order differs between languages. Put the
  whole sentence in one string with placeholders. When part of a sentence
  must be markup (a link, a badge, a bold name), split it into the fewest
  pieces that keep each piece a complete phrase, and say so in the key name
  (`deleteConfirm.before` / `deleteConfirm.after`).
- Counts use plural forms keyed by `Intl.PluralRules` categories (`zero`,
  `one`, `two`, `few`, `many`, `other`); pass `{ count }`. English needs
  `one` and `other`; Russian, Ukrainian, Croatian and Serbian also need
  `few` and `many`; translators add what their language needs.
- Keep in English: identifiers (`LIF-42`), product names (Lific, MCP,
  GitHub), keyboard keys, code, and anything that comes from the server or
  the user (issue titles, labels, comments, error messages from the API).
- Status, priority and other enum *values* are stored in English; translate
  only their display labels.

## Translating

Each `<code>.json` has exactly the keys of `en.json`, the same placeholders
and the plural categories its language uses. `bun test` checks all three.
UI tone: short, plain, the way a native product would say it; keep the
original's terseness (button labels stay one or two words).
