<script lang="ts">
  import { getContext, onMount } from "svelte";
  import { ArrowLeft } from "lucide-svelte";
  import { getArchiveCapabilities, me, onSessionChange, type ArchiveCapabilities } from "../lib/api";
  import { archiveImport, restoreArchiveImport, startArchiveImport, resetArchiveImport } from "../lib/archiveImport.svelte";
  import { t, locale, formatNumber } from "../lib/i18n";

  let { navigate }: { navigate: (path: string) => void } = $props();
  const topbarCtx = getContext<{ set: (s: import("svelte").Snippet | undefined) => void } | undefined>("lific:topbar");
  $effect(() => { topbarCtx?.set(topbarContent); return () => topbarCtx?.set(undefined); });
  let capabilities = $state<ArchiveCapabilities | null>(null);
  let loading = $state(true);
  let accessError = $state("");
  let file = $state<File | null>(null);
  let validation = $state("");
  let confirmed = $state(false);
  const busy = $derived(archiveImport.phase === "uploading" || archiveImport.phase === "processing");
  const result = $derived(archiveImport.result);
  const totalRows = $derived(result ? Object.values(result.report.rows).reduce((sum, n) => sum + n, 0) : 0);
  const mib = (bytes: number) => t("archive.import.mib", { size: (bytes / 1024 / 1024).toLocaleString(locale()) });

  onMount(() => {
    let generation = 0;
    async function initialize() {
      const current = ++generation;
      const token = localStorage.getItem("lific_token");
      loading = true; capabilities = null; accessError = "";
      file = null; confirmed = false; validation = "";
      if (!token) { accessError = t("archive.import.signIn"); loading = false; return; }
      const [res, user] = await Promise.all([getArchiveCapabilities(), me()]);
      if (current !== generation || token !== localStorage.getItem("lific_token")) return;
      if (res.ok && user.ok && token) {
        restoreArchiveImport(user.data.id, token);
        capabilities = res.data;
      } else accessError = !res.ok ? res.error : !user.ok ? user.error : t("archive.import.signIn");
      loading = false;
    }
    void initialize();
    const unsubscribe = onSessionChange(() => { void initialize(); });
    return () => { generation++; unsubscribe(); };
  });

  function selectFile(event: Event) {
    if (archiveImport.phase === "error") resetArchiveImport();
    file = (event.currentTarget as HTMLInputElement).files?.[0] ?? null;
    confirmed = false; validation = "";
    if (!file || !capabilities) return;
    if (!file.name.toLowerCase().endsWith(".tar.gz")) validation = t("archive.import.wrongType");
    else if (!file.size) validation = t("archive.import.empty");
    else if (file.size > capabilities.max_upload_bytes) validation = t("archive.import.tooLarge", { limit: mib(capabilities.max_upload_bytes) });
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!capabilities?.can_import || !file || validation || !confirmed || busy) return;
    void startArchiveImport(file);
  }
</script>

<div class="h-full overflow-y-auto">
  <div class="mx-auto w-full max-w-[680px] px-6 py-8 space-y-5 break-words">
    <h1 class="text-xl font-semibold text-[var(--text)]">{t("archive.import.title")}</h1>
    {#if loading}
      <p role="status" class="text-body-sm text-[var(--text-muted)]">{t("archive.import.checking")}</p>
    {:else if !capabilities?.can_import}
      <p role="alert" class="text-body-sm text-[var(--text-muted)]">{accessError || t("archive.import.adminOnly")}</p>
    {:else if archiveImport.phase === "success" && result}
      <div role="status" class="space-y-2">
        <h2 class="text-body-sm font-semibold text-[var(--text)]">{t("archive.import.done.title", { identifier: result.project.identifier })}</h2>
        <p class="text-body-sm text-[var(--text-muted)]">{t("archive.import.done.private")}</p>
        <p class="text-body-sm text-[var(--text)]">{t("archive.import.done.summary", { records: t("archive.import.done.records", { count: totalRows }), files: t("archive.import.done.files", { count: result.report.blobs }) })}</p>
      </div>
      <details class="text-body-sm text-[var(--text-muted)]">
        <summary class="cursor-pointer">{t("archive.import.done.recordCounts")}</summary>
        <dl class="mt-2 space-y-1">
          {#each Object.entries(result.report.rows) as [table, count]}
            <div class="flex justify-between gap-4"><dt>{table.replaceAll("_", " ")}</dt><dd>{formatNumber(count)}</dd></div>
          {/each}
        </dl>
      </details>
      {#if result.report.external_reference_count > 0}
        <section class="space-y-2" aria-labelledby="unresolved-heading">
          <h2 id="unresolved-heading" class="text-body-sm font-semibold text-[var(--text)]">{t("archive.import.done.unresolved", { count: result.report.external_reference_count })}</h2>
          <p class="text-body-sm text-[var(--text-muted)]">{t("archive.import.done.unresolvedBody")}</p>
          <ul class="list-disc pl-5 space-y-1 text-body-sm text-[var(--text-muted)] [overflow-wrap:anywhere]">
            {#each result.report.external_references as reference}<li>{reference}</li>{/each}
          </ul>
          {#if result.report.external_reference_count > result.report.external_references.length}
            <p class="text-caption text-[var(--text-muted)]">{t("archive.import.done.showingFirst", { count: result.report.external_references.length })}</p>
          {/if}
        </section>
      {:else}<p class="text-body-sm text-[var(--text-muted)]">{t("archive.import.done.noUnresolved")}</p>{/if}
      <div class="flex flex-wrap gap-3">
        <button class="bg-[var(--accent)] text-[var(--accent-text)] rounded-md px-3 py-2 text-body-sm font-medium" onclick={() => navigate(`/${result.project.identifier}/overview`)}>{t("archive.import.done.open")}</button>
        <button class="toolbar-pill" onclick={() => { resetArchiveImport(); file = null; confirmed = false; }}>{t("archive.import.done.another")}</button>
      </div>
    {:else if archiveImport.phase === "unknown"}
      <p role="alert" class="text-body-sm text-[var(--error)]">{archiveImport.error}</p>
      <button class="toolbar-pill" onclick={() => navigate("/settings")}>{t("archive.import.checkList")}</button>
      <div><button class="text-body-sm text-[var(--text-muted)] underline" onclick={resetArchiveImport}>{t("archive.import.checkedList")}</button></div>
    {:else}
      <p class="text-body-sm text-[var(--text-muted)]">{t("archive.import.intro")}</p>
      <p class="text-body-sm text-[var(--text-muted)]">{t("archive.import.noMerge")}</p>
      <form onsubmit={submit} class="space-y-4">
        <div class="space-y-2">
          <label for="project-archive" class="block text-body-sm font-medium text-[var(--text)]">{t("archive.import.fileLabel")}</label>
          <input id="project-archive" type="file" accept=".tar.gz,application/gzip" onchange={selectFile} disabled={busy} aria-describedby="archive-limits" class="block w-full min-w-0 text-body-sm text-[var(--text-muted)] file:mr-3 file:rounded-md file:border file:border-[var(--border)] file:bg-[var(--bg-subtle)] file:px-3 file:py-2 file:text-[var(--text)]" />
          <p id="archive-limits" class="text-caption text-[var(--text-muted)]">{t("archive.import.limits", { compressed: mib(capabilities.max_upload_bytes), expanded: mib(capabilities.max_expanded_bytes) })}</p>
          {#if file}<p class="text-body-sm text-[var(--text)] [overflow-wrap:anywhere]">{file.name} ({mib(file.size)})</p>{/if}
          {#if validation}<p role="alert" class="text-body-sm text-[var(--error)]">{validation}</p>{/if}
        </div>
        <label class="flex items-start gap-2 text-body-sm text-[var(--text)]">
          <input type="checkbox" bind:checked={confirmed} disabled={busy || !file || !!validation} class="mt-1 accent-[var(--accent)]" />
          {t("archive.import.confirm")}
        </label>
        {#if archiveImport.error}<p role="alert" class="text-body-sm text-[var(--error)]">{archiveImport.error}</p>{/if}
        {#if busy}
          <div role="status" class="space-y-2 text-body-sm text-[var(--text-muted)]">
            <p>{archiveImport.phase === "processing" ? t("archive.import.importing") : archiveImport.progress === null ? t("archive.import.uploading") : t("archive.import.uploadingPercent", { percent: archiveImport.progress })}</p>
            {#if archiveImport.phase === "uploading"}<progress aria-label={t("archive.import.progressLabel")} max="100" value={archiveImport.progress ?? undefined} class="w-full accent-[var(--accent)]"></progress>{/if}
            <p>{t("archive.import.processingNote")}</p>
          </div>
        {/if}
        <button type="submit" disabled={!file || !!validation || !confirmed || busy} class="bg-[var(--accent)] text-[var(--accent-text)] rounded-md px-3 py-2 text-body-sm font-medium disabled:opacity-40 disabled:cursor-not-allowed">{busy ? t("archive.import.inProgress") : t("archive.import.submit")}</button>
      </form>
    {/if}
  </div>
</div>

{#snippet topbarContent()}
  <div class="flex flex-wrap items-center gap-3 px-6 py-2">
    <button class="toolbar-pill" onclick={() => navigate("/projects/new")}><ArrowLeft size={14} /> {t("archive.import.backToNew")}</button>
    <span class="text-body-sm text-[var(--text-muted)]">{t("archive.import.crumb")}</span>
  </div>
{/snippet}
