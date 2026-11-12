<script lang="ts">
  // Fork: the issue sidebar's "Agents" field. Who hears people's comments
  // here and why, and switches for the caller's own agents to follow the
  // issue (an issue route).
  import { Bot } from "lucide-svelte";
  import { setAgentRoute, type IssueAgents } from "./api";
  import { agentLabel, routedToIssue } from "./agents";
  import { t } from "./i18n";
  import { toast } from "./toast/toast.svelte";

  let {
    info,
    issueId,
    projectId,
    onchange,
  }: { info: IssueAgents; issueId: number; projectId: number; onchange: () => void } = $props();

  let busy = $state<number | null>(null);
  let mine = $derived(info.available.filter((a) => info.mine.includes(a.id)));
  let projectWide = $derived(info.routes.filter((r) => r.issue_id === null));

  function reasons(list: string[]): string {
    return list.map((r) => t(`agents.reason.${r}`)).join(", ");
  }

  async function toggle(agentId: number, on: boolean) {
    busy = agentId;
    const res = await setAgentRoute({ agent_id: agentId, project_id: projectId, issue_id: issueId, on });
    busy = null;
    if (!res.ok) toast(res.error, { kind: "error" });
    onchange();
  }
</script>

<div class="flex flex-col gap-1.5" data-testid="issue-agents">
  {#if info.attached.length > 0}
    {#each info.attached as a (a.agent.id)}
      <div class="flex items-start gap-1.5 text-body-sm text-[var(--text)] min-w-0">
        <Bot size={13} class="shrink-0 mt-0.5 text-[var(--text-faint)]" />
        <span class="min-w-0">
          <span class="block truncate">{agentLabel(a.agent)}</span>
          <span class="block text-micro text-[var(--text-faint)]">{reasons(a.reasons)}</span>
        </span>
      </div>
    {/each}
  {:else if projectWide.length > 0}
    <p class="text-caption text-[var(--text-muted)] m-0">
      {t("agents.projectFallback", { names: projectWide.map((r) => agentLabel(r.agent)).join(", ") })}
    </p>
  {:else}
    <p class="text-caption text-[var(--text-faint)] m-0">{t("agents.noneHere")}</p>
  {/if}

  {#if mine.length > 0}
    <div class="mt-1 flex flex-col gap-1">
      {#each mine as a (a.id)}
        {@const on = routedToIssue(info, a.id)}
        <label class="flex items-center gap-2 text-caption text-[var(--text-muted)] cursor-pointer select-none">
          <input
            type="checkbox"
            class="accent-[var(--accent)]"
            checked={on}
            disabled={busy === a.id}
            onchange={() => toggle(a.id, !on)}
          />
          <span class="truncate">{t("agents.follow", { name: a.display_name || a.username })}</span>
        </label>
      {/each}
    </div>
  {/if}
</div>
