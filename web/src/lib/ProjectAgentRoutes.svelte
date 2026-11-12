<script lang="ts">
  // Fork: project-level comment routing (agent inbox). An agent routed to
  // the project hears people's comments on issues no agent is attached to.
  // Owners route their own agents; an admin can route any.
  import { Bot } from "lucide-svelte";
  import {
    listProjectAgentRoutes,
    listProjectAgents,
    me,
    setAgentRoute,
    type AgentRef,
    type AgentRoute,
  } from "./api";
  import { agentLabel } from "./agents";
  import { t } from "./i18n";
  import { toast } from "./toast/toast.svelte";

  let { projectId }: { projectId: number } = $props();

  let agents = $state<AgentRef[]>([]);
  let routes = $state<AgentRoute[]>([]);
  let myId = $state<number | null>(null);
  let isAdmin = $state(false);
  let busy = $state<number | null>(null);

  async function load(pid: number) {
    const [a, r, u] = await Promise.all([listProjectAgents(pid), listProjectAgentRoutes(pid), me()]);
    if (a.ok) agents = a.data;
    if (r.ok) routes = r.data;
    if (u.ok) {
      myId = u.data.id;
      isAdmin = u.data.is_admin;
    }
  }

  $effect(() => {
    load(projectId);
  });

  let steerable = $derived(agents.filter((a) => isAdmin || (myId !== null && a.owner_id === myId)));
  let others = $derived(routes.filter((r) => !steerable.some((a) => a.id === r.agent.id)));

  function routed(id: number): boolean {
    return routes.some((r) => r.agent.id === id);
  }

  async function toggle(id: number) {
    busy = id;
    const res = await setAgentRoute({ agent_id: id, project_id: projectId, on: !routed(id) });
    busy = null;
    if (!res.ok) toast(res.error, { kind: "error" });
    await load(projectId);
  }
</script>

{#if agents.length > 0}
  <section class="rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4" data-testid="project-agent-routes">
    <p class="text-body-sm font-medium text-[var(--text)]">{t("agents.project.title")}</p>
    <p class="text-caption text-[var(--text-muted)] mt-0.5">{t("agents.project.body")}</p>
    <div class="mt-3 flex flex-col gap-1.5">
      {#each steerable as a (a.id)}
        <label class="flex items-center gap-2 text-body-sm text-[var(--text)] cursor-pointer select-none">
          <input
            type="checkbox"
            class="accent-[var(--accent)]"
            checked={routed(a.id)}
            disabled={busy === a.id}
            onchange={() => toggle(a.id)}
          />
          <Bot size={13} class="text-[var(--text-faint)]" />
          <span class="truncate">{agentLabel(a)}</span>
        </label>
      {/each}
      {#each others as r (r.id)}
        <p class="flex items-center gap-2 text-body-sm text-[var(--text-muted)] m-0">
          <Bot size={13} class="text-[var(--text-faint)]" />
          <span class="truncate">{agentLabel(r.agent)}</span>
          <span class="text-caption text-[var(--text-faint)]">{t("agents.project.routedByOwner")}</span>
        </p>
      {/each}
      {#if steerable.length === 0 && others.length === 0}
        <p class="text-caption text-[var(--text-faint)] m-0">{t("agents.project.none")}</p>
      {/if}
    </div>
  </section>
{/if}
