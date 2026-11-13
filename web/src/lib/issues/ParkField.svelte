<script lang="ts">
  // Fork: park an issue or bring it back, on the issue page. A parked issue
  // shows when, by whom and why; an open one offers "Park" with an
  // optional reason. Closed issues cannot be parked, so the field hides.
  import { t } from "../i18n";
  import { parkIssue, unparkIssue, type Issue, type IssueParked } from "../api";
  import { toast } from "../toast/toast.svelte";
  import TimeAgo from "../TimeAgo.svelte";
  import { Hourglass, Undo2 } from "lucide-svelte";

  let {
    issue,
    editable,
    variant = "field",
    onChange,
  }: {
    issue: Issue;
    editable: boolean;
    /** `banner`: the strip above the description, only while parked. */
    variant?: "field" | "banner";
    onChange: (parked: IssueParked | undefined) => void;
  } = $props();

  let open = $state(false);
  let reason = $state("");
  let busy = $state(false);

  const closed = $derived(issue.status === "done" || issue.status === "cancelled");

  async function park() {
    busy = true;
    const res = await parkIssue(issue.id, reason);
    busy = false;
    if (!res.ok) {
      toast(t("parked.toast.failed", { error: res.error }), { kind: "error" });
      return;
    }
    open = false;
    reason = "";
    onChange(res.data);
    toast(t("parked.toast.parkedOne"), { kind: "success" });
  }

  async function unpark() {
    busy = true;
    const res = await unparkIssue(issue.id);
    busy = false;
    if (!res.ok) {
      toast(t("parked.toast.failed", { error: res.error }), { kind: "error" });
      return;
    }
    onChange(undefined);
    toast(t("parked.toast.backOne"), { kind: "success" });
  }
</script>

{#snippet note(p: IssueParked)}
  <span>
    <TimeAgo date={p.at} />{#if p.by_name}{" · "}{p.by_name}{/if}
  </span>
  {#if p.reason}
    <span class="text-[var(--text)] break-words">{p.reason}</span>
  {/if}
{/snippet}

{#if variant === "banner"}
  {#if issue.parked}
    <div
      class="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-3 py-2 rounded-lg
             bg-[var(--bg-subtle)] border border-[var(--border)] text-body-sm text-[var(--text-muted)]"
      data-testid="issue-parked-banner"
    >
      <Hourglass size={15} class="shrink-0 text-[var(--text-faint)]" />
      <span class="font-medium text-[var(--text)]">{t("parked.issue.title")}</span>
      <span class="flex flex-wrap gap-x-2 min-w-0">{@render note(issue.parked)}</span>
      {#if editable}
        <button
          class="ml-auto flex items-center gap-1 text-body-sm font-medium px-2.5 py-1 rounded-md
                 text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)]
                 disabled:opacity-50"
          disabled={busy}
          data-testid="issue-unpark"
          onclick={unpark}
        >
          <Undo2 size={13} />
          {t("parked.action.unpark")}
        </button>
      {/if}
    </div>
  {/if}
{:else if issue.parked || (editable && !closed)}
  <div class="flex flex-col gap-1.5 py-1" data-testid="issue-park-field">
    {#if issue.parked}
      <div class="flex items-center gap-1.5 text-body-sm text-[var(--text)]">
        <Hourglass size={14} class="text-[var(--text-faint)]" />
        {t("parked.issue.title")}
      </div>
      <div class="flex flex-col gap-0.5 text-caption text-[var(--text-muted)]">
        {@render note(issue.parked)}
      </div>
    {:else if !open}
      <button
        class="self-start flex items-center gap-1.5 text-body-sm px-2 py-1 -mx-2 rounded-md
               text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors"
        data-testid="issue-park"
        onclick={() => (open = true)}
      >
        <Hourglass size={14} />
        {t("parked.action.park")}
      </button>
      <p class="text-caption text-[var(--text-faint)]">{t("parked.issue.hint")}</p>
    {:else}
      <form
        class="flex flex-col gap-2"
        onsubmit={(e) => {
          e.preventDefault();
          park();
        }}
      >
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="w-full text-body-sm px-2 py-1.5 rounded-md bg-[var(--bg)] border border-[var(--border)]
                 text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:border-[var(--accent)]"
          placeholder={t("parked.reasonPlaceholder")}
          maxlength="500"
          autofocus
          bind:value={reason}
          data-testid="issue-park-reason"
          onkeydown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              open = false;
            }
          }}
        />
        <div class="flex gap-2 justify-end">
          <button
            type="button"
            class="text-body-sm px-2.5 py-1 rounded-md text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
            onclick={() => (open = false)}>{t("common.cancel")}</button
          >
          <button
            type="submit"
            class="text-body-sm font-medium px-2.5 py-1 rounded-md
                   text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)]
                   disabled:opacity-50"
            disabled={busy}
            data-testid="issue-park-confirm">{t("parked.action.park")}</button
          >
        </div>
      </form>
    {/if}
  </div>
{/if}
