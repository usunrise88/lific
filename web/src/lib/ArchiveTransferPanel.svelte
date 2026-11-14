<script lang="ts">
  import { downloadProjectArchive, getArchiveCapabilities, me, onSessionChange, saveArchiveDownload } from "./api";
  import { Download } from "lucide-svelte";
  import { t } from "./i18n";

  let { projectId, identifier }: { projectId: number; identifier: string } = $props();
  let eligible = $state(false);
  let confirmed = $state(false);
  let busy = $state(false);
  let error = $state("");
  let generation = 0;
  let userId: number | null = null;
  let session: string | null = null;
  let controller: AbortController | null = null;

  $effect(() => {
    const id = projectId;
    const ident = identifier;
    async function initialize() {
      const current = ++generation;
      controller?.abort(); controller = null;
      eligible = false; confirmed = false; busy = false; error = ""; userId = null;
      session = localStorage.getItem("lific_token");
      const token = session;
      if (!token) return;
      const [caps, user] = await Promise.all([getArchiveCapabilities(), me()]);
      if (current !== generation || id !== projectId || ident !== identifier || token !== localStorage.getItem("lific_token")) return;
      if (caps.ok && user.ok) { userId = user.data.id; eligible = true; }
    }
    void initialize();
    const unsubscribe = onSessionChange(() => { void initialize(); });
    return () => { generation++; controller?.abort(); unsubscribe(); };
  });

  async function download() {
    if (!eligible || !confirmed || busy || userId === null || !session) return;
    const current = generation;
    const id = projectId, ident = identifier, owner = userId, token = session;
    const live = () => current === generation && id === projectId && ident === identifier
      && owner === userId && token === session && token === localStorage.getItem("lific_token");
    if (!live()) return;
    controller = new AbortController();
    busy = true; error = "";
    const result = await downloadProjectArchive(ident, controller.signal);
    if (!live()) return;
    if (result.ok) {
      const user = await me();
      if (!live()) return;
      if (user.ok && user.data.id === owner) saveArchiveDownload(result.blob, result.filename);
      else error = t("archive.export.accountChanged");
    }
    if (!result.ok) error = result.error;
    busy = false;
  }
</script>

{#if eligible}
  <section aria-labelledby="archive-transfer-heading" class="border-t border-[var(--border)] pt-6 space-y-3">
    <h2 id="archive-transfer-heading" class="text-body-sm font-semibold text-[var(--text)]">{t("archive.export.title")}</h2>
    <p class="text-body-sm text-[var(--text-muted)]">
      {t("archive.export.intro")}
    </p>
    <p class="text-body-sm text-[var(--text-muted)]">{t("archive.export.untouched")}</p>
    <label class="flex items-start gap-2 text-body-sm text-[var(--text)]">
      <input type="checkbox" bind:checked={confirmed} disabled={busy} class="mt-1 accent-[var(--accent)]" />
      {t("archive.export.confirm")}
    </label>
    {#if error}<p role="alert" class="text-body-sm text-[var(--error)] break-words">{error}</p>{/if}
    <button class="toolbar-pill disabled:opacity-40" disabled={!confirmed || busy} onclick={download}>
      <Download size={14} /> {busy ? t("archive.export.preparing") : t("archive.export.download")}
    </button>
  </section>
{/if}
