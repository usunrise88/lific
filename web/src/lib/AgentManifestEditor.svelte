<script lang="ts">
  // Fork: the instance's agent manifest, edited by an admin. Agents fetch it
  // rendered (get_manifest, the `manifest` MCP prompt, /api/agent/manifest)
  // and keep it between the lific:begin/end markers of AGENTS.md or
  // CLAUDE.md; get_briefing tells them when it changed.
  import { AlertTriangle, Bot, Check, Copy, Eye, Lock, Pencil, RotateCcw } from "lucide-svelte";
  import {
    getAgentManifest,
    resetAgentManifest,
    saveAgentManifest,
    type AgentManifest,
    type RequestResult,
  } from "./api";
  import { formatNumber, t } from "./i18n";
  import Markdown from "./Markdown.svelte";
  import {
    RECENT_AUTH_ERROR,
    needsReauth,
    reauthenticateWithPassword,
    reauthenticateWithoutPassword,
    retryOnceAfterReauth,
  } from "./reauth";
  import TimeAgo from "./TimeAgo.svelte";
  import { toast } from "./toast/toast.svelte";

  let { userId, autoLogin }: { userId: number; autoLogin: boolean } = $props();

  let manifest = $state<AgentManifest | null>(null);
  let loadError = $state("");
  let draft = $state("");
  let mode = $state<"edit" | "preview">("edit");
  let busy = $state(false);
  let error = $state("");
  /** The write a fresh sign-in would resume: save the draft, or reset. */
  let parked = $state<"save" | "reset" | null>(null);
  let password = $state("");

  async function load() {
    const res = await getAgentManifest();
    if (!res.ok) {
      loadError = res.error;
      return;
    }
    manifest = res.data;
    draft = res.data.content;
  }
  load();

  let dirty = $derived(manifest !== null && draft.trim() !== manifest.content.trim());
  let chars = $derived([...draft].length);
  let tooLong = $derived(manifest !== null && chars > manifest.max_chars);
  let origin = $derived(typeof location === "undefined" ? "" : location.origin);
  let previewText = $derived(draft.replaceAll("{{project}}", "APP"));

  const promptLine = "/mcp__lific__manifest";
  let agentLine = $derived(t("manifest.howAgentText"));
  let curlLine = $derived(
    `curl -s -H "Authorization: Bearer $LIFIC_API_KEY" "${origin}/api/agent/manifest?project=APP"`,
  );

  function autoReauth() {
    return autoLogin
      ? reauthenticateWithoutPassword(userId)
      : Promise.resolve({ ok: false as const, error: RECENT_AUTH_ERROR, recoverable: true });
  }

  function send(op: "save" | "reset"): Promise<RequestResult<AgentManifest>> {
    return op === "save" ? saveAgentManifest(draft) : resetAgentManifest();
  }

  function applied(m: AgentManifest, op: "save" | "reset") {
    manifest = m;
    draft = m.content;
    parked = null;
    error = "";
    toast(op === "save" ? t("manifest.saved", { version: m.version }) : t("manifest.wasReset"));
  }

  async function run(op: "save" | "reset") {
    if (busy) return;
    if (op === "reset" && !confirm(t("manifest.resetConfirm"))) return;
    busy = true;
    error = "";
    const res = await retryOnceAfterReauth(() => send(op), autoReauth);
    busy = false;
    if (res.ok) applied(res.data, op);
    else if (needsReauth(res)) parked = op;
    else error = res.error;
  }

  async function confirmPassword() {
    const op = parked;
    if (!op || busy || !password) return;
    busy = true;
    error = "";
    const refreshed = await reauthenticateWithPassword(password, userId);
    if (!refreshed.ok) {
      error = refreshed.error;
      busy = false;
      return;
    }
    password = "";
    const res = await send(op);
    busy = false;
    if (res.ok) applied(res.data, op);
    else error = needsReauth(res) ? t("instance.reauth.stillRejected") : res.error;
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast(t("common.copied"));
    } catch {
      toast(t("manifest.copyFailed"), { kind: "error" });
    }
  }
</script>

<section class="mt-10 animate-reveal delay-200" data-testid="agent-manifest">
  <div class="flex items-center gap-2 mb-1">
    <Bot size={16} class="text-[var(--text-muted)]" />
    <h2 class="text-[1rem] font-semibold text-[var(--text)]">{t("manifest.title")}</h2>
  </div>
  <p class="text-body text-[var(--text-muted)] mb-5 leading-relaxed max-w-[70ch]">{t("manifest.intro")}</p>

  {#if loadError}
    <p class="text-caption text-[var(--error)]" role="alert">{loadError}</p>
  {:else if manifest}
    <div class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] overflow-hidden">
      <div class="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--border)] flex-wrap">
        <div class="flex rounded-md border border-[var(--border)] overflow-hidden text-body-sm" role="tablist">
          <button
            role="tab"
            aria-selected={mode === "edit"}
            class="flex items-center gap-1.5 px-2.5 py-1 {mode === 'edit' ? 'bg-[var(--bg-subtle)] text-[var(--text)]' : 'text-[var(--text-muted)]'}"
            onclick={() => (mode = "edit")}
          ><Pencil size={12} />{t("manifest.edit")}</button>
          <button
            role="tab"
            aria-selected={mode === "preview"}
            class="flex items-center gap-1.5 px-2.5 py-1 border-l border-[var(--border)] {mode === 'preview' ? 'bg-[var(--bg-subtle)] text-[var(--text)]' : 'text-[var(--text-muted)]'}"
            onclick={() => (mode = "preview")}
          ><Eye size={12} />{t("manifest.preview")}</button>
        </div>
        <span class="text-caption text-[var(--text-faint)] flex items-center gap-1.5 flex-wrap">
          <span class="font-mono">v={manifest.version}</span>
          ·
          {#if manifest.is_default}
            {t("manifest.isDefault")}
          {:else if manifest.updated_at}
            {t("manifest.edited")} <TimeAgo date={manifest.updated_at} />
          {/if}
        </span>
        <span class="ml-auto text-caption {tooLong ? 'text-[var(--error)]' : 'text-[var(--text-faint)]'}">
          {t("manifest.chars", { count: chars, max: formatNumber(manifest.max_chars) })}
        </span>
      </div>

      {#if mode === "edit"}
        <textarea
          bind:value={draft}
          spellcheck="false"
          aria-label={t("manifest.title")}
          class="block w-full min-h-[360px] px-4 py-3 font-mono text-[0.8125rem] leading-relaxed
                 bg-[var(--surface)] text-[var(--text)] outline-none resize-y"
          disabled={busy || parked !== null}
        ></textarea>
      {:else}
        <div class="px-4 py-3 min-h-[360px] text-body">
          <Markdown content={previewText} />
        </div>
      {/if}

      <div class="flex items-center gap-2 px-4 py-2.5 border-t border-[var(--border)] flex-wrap">
        <span class="text-caption text-[var(--text-faint)]">
          {t("manifest.placeholderHint.before")} <code class="font-mono text-[var(--text-muted)]">{"{{project}}"}</code>
          {t("manifest.placeholderHint.after")}
        </span>
        <span class="ml-auto flex items-center gap-2">
          {#if !manifest.is_default}
            <button
              class="flex items-center gap-1.5 text-body-sm text-[var(--text-muted)] px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] disabled:opacity-40"
              disabled={busy || parked !== null}
              onclick={() => run("reset")}
            ><RotateCcw size={13} />{t("manifest.reset")}</button>
          {/if}
          {#if dirty}
            <button
              class="text-body-sm text-[var(--text-muted)] px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] disabled:opacity-40"
              disabled={busy || parked !== null}
              onclick={() => (draft = manifest?.content ?? "")}
            >{t("manifest.discard")}</button>
          {/if}
          <button
            class="flex items-center gap-1.5 text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)]
                   px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            disabled={!dirty || tooLong || busy || parked !== null || !draft.trim()}
            onclick={() => run("save")}
          ><Check size={13} />{busy ? t("common.saving") : t("common.save")}</button>
        </span>
      </div>

      {#if error && !parked}
        <p class="px-4 pb-3 text-caption text-[var(--error)] flex items-center gap-1" role="alert">
          <AlertTriangle size={12} /> {error}
        </p>
      {/if}

      {#if parked}
        <div class="px-4 py-3 border-t border-[var(--border)] bg-[var(--bg-subtle)] flex flex-col gap-2">
          <p class="flex items-start gap-2 text-caption text-[var(--text)] leading-relaxed m-0">
            <Lock size={13} class="shrink-0 mt-0.5 text-[var(--text-muted)]" />
            {t("manifest.reauth")}
          </p>
          {#if error}
            <p class="text-caption text-[var(--error)] flex items-center gap-1 m-0" role="alert">
              <AlertTriangle size={12} /> {error}
            </p>
          {/if}
          <div class="flex items-center gap-2 flex-wrap">
            <label class="flex-1 min-w-[180px]">
              <span class="sr-only">{t("instance.reauth.passwordLabel")}</span>
              <input
                bind:value={password}
                type="password"
                placeholder={t("instance.reauth.passwordPlaceholder")}
                autocomplete="current-password"
                disabled={busy}
                class="w-full px-3 py-1.5 text-body-sm rounded-md border border-[var(--border)] bg-[var(--bg)] text-[var(--text)]
                       outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] disabled:opacity-50"
                onkeydown={(e) => { if (e.key === "Enter") confirmPassword(); }}
              />
            </label>
            <button
              class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md
                     hover:bg-[var(--btn-success-hover)] disabled:opacity-40 disabled:cursor-not-allowed shrink-0"
              disabled={busy || !password}
              onclick={confirmPassword}
            >{busy ? t("instance.reauth.verifying") : t("instance.reauth.submit")}</button>
            <button
              class="text-body-sm text-[var(--text-muted)] px-3 py-1.5 rounded-md hover:bg-[var(--surface)] disabled:opacity-50 shrink-0"
              disabled={busy}
              onclick={() => { parked = null; password = ""; error = ""; }}
            >{t("common.cancel")}</button>
          </div>
        </div>
      {/if}
    </div>

    <div class="mt-4 rounded-xl border border-[var(--border)] px-4 py-3">
      <p class="text-body-sm font-medium text-[var(--text)] m-0">{t("manifest.howTitle")}</p>
      <p class="text-caption text-[var(--text-muted)] mt-0.5 mb-3 leading-relaxed">{t("manifest.howIntro")}</p>
      <dl class="flex flex-col gap-2.5 m-0">
        {#each [
          { label: t("manifest.howClaude"), text: promptLine },
          { label: t("manifest.howAgent"), text: agentLine },
          { label: t("manifest.howHttp"), text: curlLine },
        ] as row (row.label)}
          <div class="min-w-0">
            <dt class="text-caption text-[var(--text-faint)]">{row.label}</dt>
            <dd class="m-0 mt-0.5 flex items-start gap-2 min-w-0">
              <code class="flex-1 min-w-0 font-mono text-[0.75rem] text-[var(--text)] bg-[var(--bg-subtle)] rounded px-2 py-1 break-all">{row.text}</code>
              <button
                class="shrink-0 p-1.5 rounded-md text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)]"
                aria-label={t("common.copy")}
                title={t("common.copy")}
                onclick={() => copy(row.text)}
              ><Copy size={13} /></button>
            </dd>
          </div>
        {/each}
      </dl>
      <p class="text-caption text-[var(--text-faint)] mt-3 mb-0 leading-relaxed">{t("manifest.howRefresh")}</p>
    </div>
  {/if}
</section>
