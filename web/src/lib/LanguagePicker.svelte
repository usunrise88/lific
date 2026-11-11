<script lang="ts">
  // Fork: the interface language. Each language is listed in its own name;
  // the choice is kept per browser (lib/i18n).
  import { Languages } from "lucide-svelte";
  import { LOCALES, locale, setLocale, t } from "./i18n";

  let { compact = false }: { compact?: boolean } = $props();

  let busy = $state(false);

  async function change(e: Event) {
    const code = (e.currentTarget as HTMLSelectElement).value;
    busy = true;
    await setLocale(code);
    busy = false;
  }
</script>

<label
  class="inline-flex items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--surface)]
         focus-within:ring-2 focus-within:ring-[var(--accent)] {compact ? 'px-2 py-1' : 'px-2.5 py-1.5'}"
  title={t("common.language")}
>
  <Languages size={compact ? 13 : 15} class="shrink-0 text-[var(--text-muted)]" />
  <span class="sr-only">{t("common.language")}</span>
  <select
    class="bg-transparent outline-none text-[var(--text)] {compact ? 'text-caption' : 'text-body-sm'} pr-1"
    value={locale()}
    disabled={busy}
    onchange={change}
  >
    {#each LOCALES as l (l.code)}
      <option value={l.code} lang={l.code}>{l.name}</option>
    {/each}
  </select>
</label>
