<script lang="ts">
  // Fork: time through the process. For issues closed in the period: lead
  // and cycle time, how long each status held them (median and 85th
  // percentile), how much of that was in a person's hands, and the lists an
  // agent supervisor acts on: work stuck in progress, issues waiting on
  // people, and the slowest closes.
  import { Bot, Clock, Hand, Hourglass, Timer, TriangleAlert, User, Zap } from "lucide-svelte";
  import type { FlowIssue, FlowReport, FlowStage } from "../api";
  import { t, formatNumber } from "../i18n";
  import { projectCodeOf } from "../references";
  import { actorName, formatDuration, isAgent, isStale, share } from "./flow";
  import TimeAgo from "../TimeAgo.svelte";

  let { report, navigate }: { report: FlowReport; navigate: (path: string) => void } = $props();

  const STAGE_COLOR: Record<FlowStage, string> = {
    backlog: "color-mix(in oklab, var(--text-faint) 45%, transparent)",
    todo: "color-mix(in oklab, var(--text-muted) 70%, transparent)",
    active: "var(--accent)",
    people: "var(--warn)",
  };
  const SYSTEM = t("flow.system");

  let stages = $derived(report.stages.filter((s) => s.stage !== "people"));
  let people = $derived(report.stages.find((s) => s.stage === "people"));
  let stageTotal = $derived(stages.reduce((n, s) => n + s.total, 0));
  let leadTotal = $derived((report.lead.mean ?? 0) * report.lead.count);
  let scaleMax = $derived(Math.max(1, ...report.stages.map((s) => s.p85 ?? 0)));
  let histMax = $derived(Math.max(1, ...report.lead_histogram.map((b) => b.count)));

  function stageLabel(stage: FlowStage): string {
    return t(`flow.stage.${stage}`);
  }

  function open(issue: FlowIssue) {
    navigate(`/${projectCodeOf(issue.identifier)}/issues/${issue.identifier}`);
  }

  function name(id: number | null): string {
    return actorName(report.actors, id, SYSTEM);
  }
</script>

{#snippet kpi(Icon: typeof Bot, label: string, value: string, sub: string, tone = "")}
  <div class="min-w-0 rounded-lg bg-[var(--bg-subtle)] px-3 py-2.5">
    <p class="flex items-center gap-1.5 text-micro uppercase tracking-wider text-[var(--text-faint)] font-medium">
      <Icon size={12} class="shrink-0" />
      <span class="truncate">{label}</span>
    </p>
    <p class="mt-1 font-display text-[1.5rem] leading-none tabular-nums {tone || 'text-[var(--text)]'}">{value}</p>
    <p class="mt-1 text-caption text-[var(--text-muted)] line-clamp-2">{sub}</p>
  </div>
{/snippet}

{#snippet issueList(Icon: typeof Bot, title: string, hint: string, total: number, items: FlowIssue[], kind: "active" | "waiting" | "slowest")}
  <div class="min-w-0 flex flex-col">
    <div class="flex items-center gap-2 px-4 pt-3 pb-1.5">
      <Icon size={13} class="shrink-0 text-[var(--text-faint)]" />
      <h3 class="text-caption font-semibold text-[var(--text)]">{title}</h3>
      <span class="text-micro tabular-nums text-[var(--text-faint)]">{total}</span>
    </div>
    <p class="px-4 pb-1.5 text-micro text-[var(--text-faint)]">{hint}</p>
    {#if items.length === 0}
      <p class="px-4 py-3 text-caption text-[var(--text-muted)]">{t(`flow.list.${kind}.empty`)}</p>
    {:else}
      {#each items as issue (issue.issue_id)}
        {@const stale = kind === "active" && isStale(issue.last_activity)}
        <button
          type="button"
          class="w-full flex items-center gap-2 px-4 py-1.5 text-left hover:bg-[var(--bg-subtle)] transition-colors min-w-0"
          data-identifier={issue.identifier}
          onclick={() => open(issue)}
        >
          <span class="text-caption font-mono text-[var(--text-faint)] shrink-0 w-[76px] truncate">{issue.identifier}</span>
          <span class="flex-1 min-w-0 flex flex-col">
            <span class="text-body-sm text-[var(--text)] truncate">{issue.title}</span>
            <span class="flex items-center gap-1 text-micro text-[var(--text-faint)] min-w-0">
              {#if issue.actor_id !== null || kind !== "waiting"}
                {#if isAgent(report.actors, issue.actor_id)}<Bot size={10} class="shrink-0" />{:else}<User size={10} class="shrink-0" />{/if}
                <span class="truncate">{name(issue.actor_id)}</span>
              {/if}
              {#if issue.last_activity && kind !== "slowest"}
                <span class="shrink-0">·</span>
                <span class="shrink-0 {stale ? 'text-[var(--warn)]' : ''}">
                  <TimeAgo date={issue.last_activity} />
                </span>
              {/if}
            </span>
          </span>
          <span
            class="shrink-0 text-caption tabular-nums px-1.5 py-0.5 rounded-md
                   {stale ? 'text-[var(--warn)] bg-[color-mix(in_oklab,var(--warn)_14%,transparent)]' : 'text-[var(--text-muted)]'}"
            title={stale ? t("flow.list.active.stale") : undefined}
          >
            {#if stale}<TriangleAlert size={10} class="inline -mt-px mr-0.5" />{/if}{formatDuration(issue.seconds)}
          </span>
        </button>
      {/each}
    {/if}
  </div>
{/snippet}

<section class="rounded-xl bg-[var(--surface)] border border-[var(--border)] min-w-0" data-testid="flow-stages">
  <header class="px-5 pt-4">
    <h2 class="text-body-sm font-semibold text-[var(--text)]">{t("flow.stages.title")}</h2>
    <p class="text-caption text-[var(--text-faint)] mt-0.5">{t("flow.stages.subtitle", { count: report.timed })}</p>
  </header>

  <div class="grid grid-cols-2 lg:grid-cols-4 gap-2 px-5 pt-4">
    {@render kpi(
      Clock,
      t("flow.kpi.lead"),
      formatDuration(report.lead.p50),
      t("flow.kpi.p85", { value: formatDuration(report.lead.p85) }),
    )}
    {@render kpi(
      Zap,
      t("flow.kpi.cycle"),
      formatDuration(report.cycle.p50),
      t("flow.kpi.p85", { value: formatDuration(report.cycle.p85) }),
    )}
    {@render kpi(
      Hand,
      t("flow.kpi.people"),
      share(people?.total ?? 0, leadTotal) === null ? "—" : `${share(people?.total ?? 0, leadTotal)}%`,
      t("flow.kpi.peopleSub", { count: people?.issues ?? 0 }),
      (share(people?.total ?? 0, leadTotal) ?? 0) >= 30 ? "text-[var(--warn)]" : "",
    )}
    {@render kpi(
      Timer,
      t("flow.kpi.skipped"),
      share(report.skipped_active, report.timed) === null ? "—" : `${share(report.skipped_active, report.timed)}%`,
      t("flow.kpi.skippedSub", { count: report.skipped_active }),
    )}
  </div>

  {#if report.timed === 0}
    <p class="px-5 py-8 text-center text-body-sm text-[var(--text-muted)]">{t("flow.stages.empty")}</p>
  {:else}
    <div class="grid grid-cols-1 lg:grid-cols-[1fr_280px] gap-6 px-5 pt-5">
      <div class="min-w-0">
        <!-- Where the time went: share of all lead time per status -->
        <p class="text-micro uppercase tracking-wider text-[var(--text-faint)] font-medium mb-1.5">{t("flow.stages.share")}</p>
        <div class="flex h-7 rounded-md overflow-hidden gap-[2px]" role="img" aria-label={t("flow.stages.shareAria")}>
          {#each stages as s (s.stage)}
            {@const pct = share(s.total, stageTotal) ?? 0}
            {#if pct > 0}
              <span
                class="h-full flex items-center px-2 min-w-0 text-micro font-medium overflow-hidden whitespace-nowrap
                       {s.stage === 'active' ? 'text-[var(--accent-text)]' : 'text-[var(--text)]'}"
                style="flex: {s.total} 1 0; background: {STAGE_COLOR[s.stage]}"
                title="{stageLabel(s.stage)} · {pct}%"
              >
                {#if pct >= 12}{stageLabel(s.stage)} {pct}%{/if}
              </span>
            {/if}
          {/each}
        </div>
        {#if people && people.total > 0}
          {@const pct = share(people.total, leadTotal) ?? 0}
          <div class="mt-1.5 flex items-center gap-2">
            <div class="flex-1 h-1.5 rounded-full bg-[var(--bg-subtle)] overflow-hidden">
              <span class="block h-full rounded-full" style="width: {pct}%; background: {STAGE_COLOR.people}"></span>
            </div>
            <span class="text-micro text-[var(--text-muted)] whitespace-nowrap">{t("flow.stages.peopleShare", { value: `${pct}%` })}</span>
          </div>
        {/if}

        <!-- Each stage: median, with the 85th percentile as the faint tail -->
        <div class="mt-5 flex flex-col gap-3">
          {#each report.stages as s (s.stage)}
            <div class="grid grid-cols-[96px_1fr_auto] sm:grid-cols-[120px_1fr_150px] items-center gap-3">
              <span class="flex items-center gap-1.5 text-caption text-[var(--text)] min-w-0">
                <span class="size-2.5 rounded-sm shrink-0" style="background: {STAGE_COLOR[s.stage]}"></span>
                <span class="truncate">{stageLabel(s.stage)}</span>
              </span>
              <div class="relative h-3 rounded-full bg-[var(--bg-subtle)]">
                {#if s.p85}
                  <span
                    class="absolute inset-y-0 left-0 rounded-full opacity-35"
                    style="width: {Math.max(2, (s.p85 / scaleMax) * 100)}%; background: {STAGE_COLOR[s.stage]}"
                  ></span>
                {/if}
                {#if s.p50}
                  <span
                    class="absolute inset-y-0 left-0 rounded-full"
                    style="width: {Math.max(2, (s.p50 / scaleMax) * 100)}%; background: {STAGE_COLOR[s.stage]}"
                  ></span>
                {/if}
              </div>
              <span class="text-caption tabular-nums text-right whitespace-nowrap">
                <span class="text-[var(--text)] font-medium">{formatDuration(s.p50)}</span>
                <span class="text-[var(--text-faint)] hidden sm:inline"> · p85 {formatDuration(s.p85)}</span>
                <span class="text-[var(--text-faint)]"> · {formatNumber(s.issues)}</span>
              </span>
            </div>
          {/each}
          <p class="text-micro text-[var(--text-faint)]">{t("flow.stages.legend")}</p>
        </div>
      </div>

      <!-- Lead time distribution -->
      <div class="min-w-0">
        <p class="text-micro uppercase tracking-wider text-[var(--text-faint)] font-medium mb-1.5">{t("flow.stages.distribution")}</p>
        <div class="flex items-end gap-1.5 h-[112px]" role="img" aria-label={t("flow.stages.distributionAria")}>
          {#each report.lead_histogram as b (b.key)}
            <div class="flex-1 min-w-0 h-full flex flex-col justify-end items-center" title="{t(`flow.span.${b.key}`)}: {b.count}">
              <span class="text-micro tabular-nums text-[var(--text-muted)] mb-0.5">{b.count || ""}</span>
              <span
                class="block w-full rounded-t-[4px] rounded-b-[2px] {b.count ? 'bg-[var(--accent)]' : 'bg-[var(--border)]'}"
                style="height: {b.count ? Math.max(3, (b.count / histMax) * 84) : 2}px"
              ></span>
            </div>
          {/each}
        </div>
        <div class="flex gap-1.5 mt-1">
          {#each report.lead_histogram as b (b.key)}
            <span class="flex-1 min-w-0 text-center text-micro leading-tight text-[var(--text-faint)]">{t(`flow.span.${b.key}`)}</span>
          {/each}
        </div>
      </div>
    </div>
  {/if}

  <div class="mt-5 grid grid-cols-1 lg:grid-cols-3 border-t border-[var(--border)] divide-y lg:divide-y-0 lg:divide-x divide-[var(--border)] pb-2">
    {@render issueList(Zap, t("flow.list.active.title"), t("flow.list.active.hint"), report.active_total, report.active, "active")}
    {@render issueList(
      Hourglass,
      t("flow.list.waiting.title"),
      report.waiting_p50 === null ? t("flow.list.waiting.hint") : t("flow.list.waiting.median", { value: formatDuration(report.waiting_p50) }),
      report.waiting_total,
      report.waiting,
      "waiting",
    )}
    {@render issueList(Clock, t("flow.list.slowest.title"), t("flow.list.slowest.hint"), report.slowest.length, report.slowest, "slowest")}
  </div>
</section>
