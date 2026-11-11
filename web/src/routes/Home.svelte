<script lang="ts">
  // LIF-237 — Home: the landing dashboard at "/".
  //
  // LIF-507 rebuilt the body around two lanes of work: what an agent can
  // pick up (unassigned, unblocked, ranked by the same importance score as
  // the project overview) and what only a person can do (the LIF-506
  // attention groups). Above them, open work by how long it has been
  // sitting, a quiet 90-day open count, and which projects are going stale.
  // The aggregates come from one cross-project read, `getHomeOverview`, so
  // they are not truncated by the list endpoint's row cap.
  //
  // Fork: a scope bar (project groups and projects, plus a period) narrows
  // every widget, and two flow reports sit under the open-work chart:
  // closure rate per day/week and per actor, and time through each stage.

  import {
    me,
    listProjects,
    listIssues,
    listAllPages,
    listProjectActivity,
    getAttention,
    getHomeOverview,
    getHomeFlow,
    listProjectGroups,
    type Attention,
    type FlowReport,
    type ProjectGroup,
    type AuthUser,
    type HomeOverview,
    type Project,
    type Issue,
    type Page,
    type Activity,
  } from "../lib/api";
  import {
    selectActivityRate,
    type ActivityCountsReader,
    type ActivityRate,
  } from "../lib/activityRate";
  import StatusIcon from "../lib/StatusIcon.svelte";
  import PriorityIcon from "../lib/PriorityIcon.svelte";
  import Mascot from "../lib/Mascot.svelte";
  import ErrorState from "../lib/ErrorState.svelte";
  import Skeleton from "../lib/Skeleton.svelte";
  import AgeChart from "../lib/home/AgeChart.svelte";
  import OpenTrend from "../lib/home/OpenTrend.svelte";
  import ProjectFreshness from "../lib/home/ProjectFreshness.svelte";
  import DoneHeatmap from "../lib/home/DoneHeatmap.svelte";
  import HeatBar from "../lib/home/HeatBar.svelte";
  import { recordHomeVisit } from "../lib/home/lastVisit";
  import HomeFilterBar from "../lib/home/HomeFilterBar.svelte";
  import ClosureRate from "../lib/home/ClosureRate.svelte";
  import FlowStages from "../lib/home/FlowStages.svelte";
  import { inScope, loadHomeFilters, saveHomeFilters, scopeIds, type HomeFilters } from "../lib/home/flow";
  import { ageLabel, daysSince, heat, importanceScore } from "../lib/issues/importance";
  import {
    Plus,
    Command,
    Pin,
    FileText,
    Sunrise,
    Sun,
    Sunset,
    Moon,
    UserCheck,
    User,
    Hourglass,
    Bot,
    ArrowRight,
  } from "lucide-svelte";
  import { projectCodeOf } from "../lib/references";
  import { getContext, untrack } from "svelte";
  import { startAutoRefresh } from "../lib/autoRefresh.svelte";
  import { t, locale } from "../lib/i18n";

  const topbarCtx = getContext<{
    set: (s: import("svelte").Snippet | undefined) => void;
  } | undefined>("lific:topbar");

  $effect(() => {
    topbarCtx?.set(topbarContent);
    return () => topbarCtx?.set(undefined);
  });

  let {
    navigate,
    realtimeActivityCounts,
    realtimeActivityReady,
    realtimeActivityRevision,
  }: {
    navigate: (path: string) => void;
    realtimeActivityCounts: ActivityCountsReader;
    realtimeActivityReady: boolean;
    realtimeActivityRevision: number;
  } = $props();

  // Measured once per mount, so auto-refreshes keep comparing against the
  // same "last here".
  const visitSince = recordHomeVisit();
  const sinceIso = visitSince === null ? null : new Date(visitSince).toISOString();

  let user = $state<AuthUser | null>(null);
  /** Every project the caller can see; `projects` is the scoped subset. */
  let allProjects = $state<Project[]>([]);
  let groups = $state<ProjectGroup[]>([]);
  let filters = $state<HomeFilters>(loadHomeFilters());
  let flowReport = $state<FlowReport | null>(null);
  /** Scope and period of the data on screen, so a filter change reloads once. */
  let loadedKey = $state<string | null>(null);
  let scope = $derived(scopeIds(filters, groups));
  let projects = $derived(scope ? allProjects.filter((p) => scope!.includes(p.id)) : allProjects);
  /** LIF-506: what needs the signed-in person. Null until loaded or when
   *  the request fails, which hides the lane rather than the dashboard. */
  let attention = $state<Attention | null>(null);
  let overview = $state<HomeOverview | null>(null);
  /** Unassigned, workable issues across projects, ranked client-side. */
  let agentPool = $state<Issue[]>([]);
  let allPages = $state<Page[]>([]);
  let activityItems = $state<Activity[]>([]);
  let activityNow = $state(Date.now());
  let loading = $state(true);
  let error = $state("");

  // The shortest displayed window is one second, so the shared 30-second
  // relative-time clock is too coarse. Keep the finer timer local to Home.
  $effect(() => {
    const interval = setInterval(() => {
      activityNow = Date.now();
    }, 1_000);
    return () => clearInterval(interval);
  });

  $effect(() => {
    loadData();
  });

  $effect(() =>
    startAutoRefresh({
      refresh: () => loadData(false),
      isBusy: () => loading,
      realtimeDebounceMs: 750,
      realtimeMaxWaitMs: 5_000,
      shouldRefresh: (event) =>
        event.type === "resync.required" ||
        event.type.startsWith("project.") ||
        event.type.startsWith("issue."),
    }),
  );

  async function loadData(initial = true) {
    if (initial) {
      loading = true;
      error = "";
    }

    const [meRes, projRes, groupRes] = await Promise.all([me(), listProjects(), listProjectGroups()]);
    if (!meRes.ok) {
      if (initial) {
        error = meRes.error;
        loading = false;
      }
      return;
    }
    error = "";
    user = meRes.data;
    if (projRes.ok) allProjects = projRes.data;
    if (groupRes.ok) groups = groupRes.data;
    const f = untrack(() => filters);
    const ids = scopeIds(f, groupRes.ok ? groupRes.data : []);
    loadedKey = scopeKey(ids, f.days);

    // The pool is ordered by priority server-side, so the 500-row cap can
    // only ever drop the least pressing issues before ranking.
    const [attentionRes, overviewRes, poolRes, pagesRes, flowRes] = await Promise.all([
      getAttention(),
      getHomeOverview(sinceIso, ids),
      listIssues({ workable: true, assignee: "none", order_by: "priority", limit: 500 }),
      listAllPages(),
      getHomeFlow(f.days, ids),
    ]);
    attention = attentionRes.ok
      ? {
          assigned: inScope(attentionRes.data.assigned, ids),
          human: inScope(attentionRes.data.human, ids),
          waiting: inScope(attentionRes.data.waiting, ids),
        }
      : null;
    overview = overviewRes.ok ? overviewRes.data : null;
    agentPool = poolRes.ok ? inScope(poolRes.data, ids) : [];
    allPages = pagesRes.ok ? inScope(pagesRes.data, ids) : [];
    flowReport = flowRes.ok ? flowRes.data : null;

    // There is no cross-project activity feed: merge the three most
    // recently active projects' feeds client-side.
    const feeds = await Promise.all(
      activeProjectIds.slice(0, 3).map((pid) => listProjectActivity(pid, 8, 0)),
    );
    const combined = feeds.flatMap((f) => (f.ok ? f.data.items : []));
    combined.sort((a, b) => b.ts.localeCompare(a.ts));
    activityItems = combined.slice(0, 8);

    if (initial) loading = false;
  }

  function scopeKey(ids: number[] | undefined, days: number): string {
    return `${ids?.join(",") ?? "*"}|${days}`;
  }

  // A filter change reloads in place, without the skeleton.
  $effect(() => {
    const key = scopeKey(scope, filters.days);
    if (loadedKey !== null && key !== loadedKey && !untrack(() => loading)) {
      untrack(() => loadData(false));
    }
  });

  function setFilters(next: HomeFilters) {
    filters = next;
    saveHomeFilters(next);
  }

  /** Visible projects, most recently active first. */
  let activeProjectIds = $derived.by(() => {
    const last = new Map((overview?.projects ?? []).map((p) => [p.project_id, p.last_activity ?? ""]));
    return [...projects]
      .sort((a, b) =>
        (last.get(b.id) || b.updated_at).localeCompare(last.get(a.id) || a.updated_at),
      )
      .map((p) => p.id);
  });

  function projectIdent(id: number | null): string | null {
    if (id === null) return null;
    return projects.find((p) => p.id === id)?.identifier ?? null;
  }

  function issueProjectIdent(issue: Issue): string {
    return projectIdent(issue.project_id) ?? projectCodeOf(issue.identifier);
  }

  function openIssue(issue: Issue) {
    navigate(`/${issueProjectIdent(issue)}/issues/${issue.identifier}`);
  }

  // ── Agent queue ──────────────────────────────────────────────

  const QUEUE_CAP = 7;

  let agentQueue = $derived.by(() => {
    const now = Date.now();
    return agentPool
      .filter((i) => i.status !== "active" && !i.needs_human)
      .map((issue) => ({ issue, score: importanceScore(issue, now) }))
      .sort((a, b) => b.score - a.score);
  });

  // ── Only you (LIF-506) ───────────────────────────────────────
  //
  // Open issues assigned to you, marked for any person, or with a wait on
  // you, across every project you can see. Empty groups are hidden. An
  // issue can show in "Waiting on you" and in one of the other two.

  const ATTENTION_CAP = 4;
  let attentionExpanded = $state<Set<string>>(new Set());

  let attentionGroups = $derived.by(() => {
    if (!attention) return [];
    return [
      { key: "assigned", label: t("home.attention.assigned"), icon: UserCheck, items: attention.assigned },
      { key: "human", label: t("home.attention.human"), icon: User, items: attention.human },
      { key: "waiting", label: t("home.attention.waiting"), icon: Hourglass, items: attention.waiting },
    ].filter((g) => g.items.length > 0);
  });
  let attentionIssues = $derived(attentionGroups.flatMap((g) => g.items));
  let attentionTotal = $derived(new Set(attentionIssues.map((i) => i.id)).size);

  function toggleAttentionGroup(key: string) {
    const next = new Set(attentionExpanded);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    attentionExpanded = next;
  }

  // ── Since you were last here ─────────────────────────────────

  function serverTime(ts: string): number {
    return new Date(ts.replace(" ", "T") + "Z").getTime();
  }

  let sinceLine = $derived.by(() => {
    if (visitSince === null || !overview?.since) return null;
    const s = overview.since;
    const fresh = new Set(
      attentionIssues.filter((i) => serverTime(i.updated_at) > visitSince).map((i) => i.id),
    ).size;
    const parts: string[] = [];
    if (s.agents_closed && s.agents_opened) {
      parts.push(t("home.since.agentsBoth", { closed: s.agents_closed, opened: s.agents_opened }));
    } else if (s.agents_closed) {
      parts.push(t("home.since.agentsClosed", { count: s.agents_closed }));
    } else if (s.agents_opened) {
      parts.push(t("home.since.agentsOpened", { count: s.agents_opened }));
    }
    if (fresh > 0) parts.push(t("home.since.fresh", { count: fresh }));
    const when = relative(visitSince);
    if (parts.length === 0) return t("home.since.quiet", { when });
    return t("home.since.summary", { when, details: parts.join(" ") });
  });

  function relative(ts: number): string {
    const mins = Math.floor((Date.now() - ts) / 60000);
    if (mins < 60) return t("common.time.minutesAgo", { count: Math.max(1, mins) });
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return t("common.time.hoursAgo", { count: hrs });
    const days = Math.floor(hrs / 24);
    return days === 1 ? t("home.since.yesterday") : t("home.since.daysAgo", { count: days });
  }

  /** Net change in open issues over the last week of the trend. */
  let weekDelta = $derived.by(() => {
    const trend = overview?.open_trend ?? [];
    if (trend.length < 8) return 0;
    return trend[trend.length - 1].count - trend[trend.length - 8].count;
  });

  // ── Pinned pages ──────────────────────────────────────────────
  //
  // See api.ts's `listAllPages` doc comment for why this is one
  // cross-project call filtered client-side.

  let pinnedPages = $derived.by(() =>
    allPages
      .filter((p) => p.pinned && p.project_id !== null)
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
      .slice(0, 6),
  );

  // ── Activity digest ──────────────────────────────────────────

  function activityVerb(a: Activity): string {
    switch (a.action) {
      case "create":
        return a.entity_type === "comment"
          ? t("home.activity.commented")
          : t("home.activity.created", { entity: entityName(a.entity_type) });
      case "delete":
        return a.entity_type === "comment"
          ? t("home.activity.commentDeleted")
          : t("home.activity.deleted", { entity: entityName(a.entity_type) });
      case "update":
        return a.entity_type === "comment"
          ? t("home.activity.commentEdited")
          : a.field
            ? t("home.activity.changedField", { field: a.field })
            : t("home.activity.updated");
      case "attach":
        return t("home.activity.labeled");
      case "detach":
        return t("home.activity.unlabeled");
      case "link":
        return t("home.activity.linked");
      case "unlink":
        return t("home.activity.unlinked");
      case "assign":
        return t("home.activity.assigned");
      case "unassign":
        return t("home.activity.unassigned");
      case "wait":
        return t("home.activity.waitSet");
      case "unwait":
        return t("home.activity.waitCleared");
      default:
        return a.action;
    }
  }

  /** The entity type's display name; unknown types stay as sent. */
  function entityName(type: string): string {
    const key = `home.activity.entity.${type}`;
    const name = t(key);
    return name === key ? type : name;
  }

  function activityActor(a: Activity): string {
    return a.actor_display_name || a.actor_username || t("home.activity.system");
  }

  function rateLabel(rate: ActivityRate): string {
    const key = {
      "updates/s": "home.rate.perSecond",
      "updates/min": "home.rate.perMinute",
      "updates/hr": "home.rate.perHour",
      "updates/day": "home.rate.perDay",
    }[rate.unit];
    return t(key, { count: rate.value });
  }

  let activityRate = $derived.by(() => {
    void realtimeActivityRevision;
    return selectActivityRate(realtimeActivityCounts(activityNow));
  });

  function activityDest(a: Activity): string | null {
    const ident = projectIdent(a.project_id);
    if (!ident) return null;
    switch (a.entity_type) {
      case "issue":
        return a.entity_label ? `/${ident}/issues/${a.entity_label}` : null;
      case "page":
        return `/${ident}/pages/${a.entity_id}`;
      case "comment":
        if (a.issue_id !== null && a.entity_label) return `/${ident}/issues/${a.entity_label}`;
        if (a.page_id !== null) return `/${ident}/pages/${a.page_id}`;
        return null;
      default:
        return null;
    }
  }

  // ── Greeting + quick actions ────────────────────────────────────

  function greetingText(h: number): string {
    if (h < 5) return t("home.greeting.night");
    if (h < 12) return t("home.greeting.morning");
    if (h < 17) return t("home.greeting.afternoon");
    if (h < 21) return t("home.greeting.evening");
    return t("home.greeting.night");
  }

  function greetingIcon(h: number) {
    if (h < 5) return Moon;
    if (h < 12) return Sunrise;
    if (h < 17) return Sun;
    if (h < 21) return Sunset;
    return Moon;
  }

  const nowHour = new Date().getHours();
  const greeting = greetingText(nowHour);
  const GreetIcon = greetingIcon(nowHour);
  const todayLabel = new Date().toLocaleDateString(locale(), {
    weekday: "long",
    month: "long",
    day: "numeric",
  });

  // "New issue" lands in the most recently active project.
  let quickIssueProject = $derived(projectIdent(activeProjectIds[0] ?? null));

  function openPalette() {
    // The palette listens for cmd/ctrl+K globally (CommandPalette.svelte,
    // mounted once in Layout) — dispatch the same chord rather than
    // threading a ref through Layout for a single button.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  }
</script>

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-6 py-2 w-full">
    <span class="text-body-sm font-medium text-[var(--text)]">{t("home.title")}</span>
  </div>
{/snippet}

<!-- Fork: a widget's way into its full page (Feed, For me). `padded` is for
     a widget whose body already has padding. -->
{#snippet sectionLink(path: string, label: string, padded = false)}
  <a
    href="#{path}"
    class="group flex items-center justify-end gap-1 text-caption text-[var(--text-muted)] hover:text-[var(--accent)] transition-colors
           {padded ? 'mt-3 pt-2.5 border-t border-[var(--border)]' : 'px-4 py-2.5 border-t border-[var(--border)]'}"
    onclick={(e) => {
      if (!e.metaKey && !e.ctrlKey && !e.shiftKey && e.button === 0) {
        e.preventDefault();
        navigate(path);
      }
    }}
  >
    {label}
    <ArrowRight size={12} class="transition-transform group-hover:translate-x-0.5" />
  </a>
{/snippet}

{#snippet cardHeader(Icon: typeof Bot, title: string, subtitle: string, count?: number)}
  <div class="flex items-center gap-2.5 px-4 py-3 border-b border-[var(--border)]">
    <span class="size-7 shrink-0 rounded-lg bg-[var(--bg-subtle)] flex items-center justify-center text-[var(--text-muted)]">
      <Icon size={14} />
    </span>
    <div class="min-w-0 flex-1">
      <h2 class="text-body-sm font-semibold text-[var(--text)] leading-tight flex items-center gap-2">
        {title}
        {#if count !== undefined}
          <span class="text-micro font-normal text-[var(--text-faint)] tabular-nums">{count}</span>
        {/if}
      </h2>
      <p class="text-caption text-[var(--text-faint)] truncate">{subtitle}</p>
    </div>
  </div>
{/snippet}

<div class="h-full flex flex-col">
  <div class="flex-1 overflow-y-auto">
    {#if loading}
      <!-- LIF-281: content-shaped skeleton mirroring the loaded layout:
           greeting, the chart card, the two lanes, then the bottom row. -->
      <div class="max-w-[1280px] mx-auto px-6 md:px-8 py-8 md:py-10">
        <div class="flex flex-wrap items-start justify-between gap-4 mb-6">
          <div class="flex items-center gap-3">
            <Skeleton variant="circle" class="size-11 rounded-xl" />
            <div class="flex flex-col gap-2">
              <Skeleton variant="bar" class="h-5 w-48" />
              <Skeleton variant="bar" class="h-3 w-32" />
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Skeleton variant="bar" class="h-8 w-28 rounded-md" />
            <Skeleton variant="bar" class="h-8 w-24 rounded-md" />
          </div>
        </div>
        <Skeleton variant="block" class="h-[300px] w-full rounded-xl mb-6" />
        <div class="grid grid-cols-1 lg:grid-cols-[3fr_2fr] gap-6 mb-6">
          {#each [0, 1] as lane (lane)}
            <div class="rounded-xl bg-[var(--surface)] border border-[var(--border)] overflow-hidden">
              <div class="flex items-center gap-2.5 px-4 py-3 border-b border-[var(--border)]">
                <Skeleton variant="circle" class="size-7 rounded-lg" />
                <Skeleton variant="bar" class="h-3.5 w-32" />
              </div>
              {#each [0, 1, 2, 3] as row (row)}
                <div class="flex items-center gap-2.5 px-4 py-2.5 border-b border-[var(--border)] last:border-b-0">
                  <Skeleton variant="bar" class="h-3 w-16 shrink-0" />
                  <Skeleton variant="bar" class="h-3 flex-1 max-w-[260px]" />
                </div>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    {:else if error}
      <ErrorState title={t("home.error.title")} message={error}>
        <button
          class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
          onclick={() => loadData()}
        >
          {t("common.retry")}
        </button>
      </ErrorState>
    {:else}
      <div class="max-w-[1280px] mx-auto px-6 md:px-8 py-8 md:py-10 flex flex-col gap-6">
        <!-- ── GREETING + QUICK ACTIONS ─────────────────────── -->
        <div class="flex flex-wrap items-start justify-between gap-4">
          <div class="flex items-center gap-3 min-w-0">
            <span
              class="size-11 shrink-0 rounded-xl bg-[var(--accent-subtle)]
                     flex items-center justify-center text-[var(--accent)]"
            >
              <GreetIcon size={20} />
            </span>
            <div class="min-w-0">
              <h1 class="font-display text-title tracking-tight text-[var(--text)] leading-none">
                {user ? t("home.greeting.named", { greeting, name: user.display_name || user.username }) : greeting}
              </h1>
              <p class="text-body-sm text-[var(--text-muted)] mt-1">{todayLabel}</p>
            </div>
          </div>

          <div class="flex items-center gap-2">
            {#if quickIssueProject}
              <button
                class="flex items-center gap-1.5 text-body-sm font-medium
                       text-[var(--btn-success-text)] bg-[var(--btn-success)]
                       px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
                onclick={() => navigate(`/${quickIssueProject}/issues/new`)}
              >
                <Plus size={14} /> {t("home.newIssue")}
              </button>
            {/if}
            <button
              class="flex items-center gap-1.5 text-body-sm text-[var(--text-muted)]
                     hover:text-[var(--text)] border border-[var(--border)]
                     px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] transition-colors"
              onclick={openPalette}
              title={t("home.palette.title")}
            >
              <Command size={13} />
              {t("home.palette.jump")}
              <kbd class="font-mono text-micro leading-none text-[var(--text-faint)]
                          border border-[var(--border)] rounded px-1 py-0.5 ml-0.5">⌘K</kbd>
            </button>
          </div>
          {#if sinceLine}
            <p class="basis-full -mt-2 text-body-sm text-[var(--text-muted)]" data-testid="since-line">
              {sinceLine}
            </p>
          {/if}
        </div>

        <!-- ── SCOPE (fork) ─────────────────────────────────── -->
        <HomeFilterBar {filters} {groups} projects={allProjects} onchange={setFilters} />

        <!-- ── OPEN WORK BY AGE ─────────────────────────────── -->
        {#if overview}
          <section
            class="rounded-xl bg-[var(--surface)] border border-[var(--border)]
                   grid grid-cols-1 lg:grid-cols-[1fr_340px]"
          >
            <div class="p-5 min-w-0">
              <h2 class="text-body-sm font-semibold text-[var(--text)]">{t("home.age.title")}</h2>
              <p class="text-caption text-[var(--text-faint)] mt-0.5">
                {t("home.age.summary", {
                  open: overview.open_total,
                  count: overview.projects.filter((p) => p.open > 0).length,
                  delta: weekDelta > 0 ? `+${weekDelta}` : weekDelta === 0 ? t("home.age.noChange") : weekDelta,
                })}
              </p>
              <AgeChart buckets={overview.age_buckets} />
            </div>
            <div
              class="p-5 flex flex-col gap-5 border-t lg:border-t-0 lg:border-l border-[var(--border)] min-w-0"
            >
              <div>
                <div class="flex items-baseline justify-between mb-2">
                  <h2 class="text-body-sm font-semibold text-[var(--text)]">{t("home.trend.title")}</h2>
                  <span class="text-micro text-[var(--text-faint)] tabular-nums">
                    {t("home.trend.now", { count: overview.open_trend[overview.open_trend.length - 1]?.count ?? 0 })}
                  </span>
                </div>
                <OpenTrend points={overview.open_trend} />
              </div>
              <div>
                <h2 class="text-body-sm font-semibold text-[var(--text)] mb-1.5">{t("home.freshness.title")}</h2>
                <ProjectFreshness pulses={overview.projects} {projects} {navigate} />
              </div>
            </div>
          </section>
        {/if}

        <!-- ── FLOW REPORTS (fork) ──────────────────────────── -->
        {#if flowReport}
          <ClosureRate report={flowReport} />
          <FlowStages report={flowReport} {navigate} />
        {/if}

        <!-- ── THE TWO LANES ─────────────────────────────────── -->
        <div class="grid grid-cols-1 lg:grid-cols-[3fr_2fr] gap-6 items-start">
          <section
            class="rounded-xl bg-[var(--surface)] border border-[var(--border)] overflow-hidden min-w-0"
            data-testid="agent-queue"
          >
            {@render cardHeader(
              Bot,
              t("home.queue.title"),
              t("home.queue.subtitle"),
              agentQueue.length,
            )}
            {#if agentQueue.length === 0}
              <div class="flex flex-col items-center justify-center py-12 gap-3">
                <Mascot src="/LizzySleep2.png" nativeW={1000} nativeH={420} scale={0.16} />
                <p class="text-body-sm text-[var(--text-muted)]">{t("home.queue.empty")}</p>
              </div>
            {:else}
              {#each agentQueue.slice(0, QUEUE_CAP) as { issue, score }, rank (issue.id)}
                {@const idle = daysSince(issue.updated_at)}
                <button
                  class="w-full flex items-center gap-3 px-4 py-2.5 text-left border-b border-[var(--border)]
                         last:border-b-0 hover:bg-[var(--bg-subtle)] transition-colors"
                  data-identifier={issue.identifier}
                  onclick={() => openIssue(issue)}
                >
                  <span class="w-4 shrink-0 text-caption tabular-nums text-[var(--text-faint)] text-right">{rank + 1}</span>
                  <span class="w-14 shrink-0 hidden sm:block"><HeatBar value={heat(score)} /></span>
                  <span class="text-caption font-mono text-[var(--text-faint)] w-[64px] shrink-0 truncate">
                    {issue.identifier}
                  </span>
                  <span class="text-body-sm text-[var(--text)] truncate flex-1">{issue.title}</span>
                  <PriorityIcon priority={issue.priority} size={15} />
                  <span class="shrink-0 hidden md:flex items-center gap-1.5 text-micro tabular-nums">
                    <span class="w-[52px] text-right text-[var(--text-faint)]">{t("home.queue.old", { age: ageLabel(daysSince(issue.created_at)) })}</span>
                    {#if idle >= 14}
                      <span class="px-1.5 py-0.5 rounded-full text-[var(--warn)] bg-[color-mix(in_oklab,var(--warn)_14%,transparent)]">
                        {t("home.queue.idle", { age: ageLabel(idle) })}
                      </span>
                    {:else}
                      <span class="px-1.5 py-0.5 text-[var(--text-faint)]">{t("home.queue.idle", { age: ageLabel(idle) })}</span>
                    {/if}
                  </span>
                </button>
              {/each}
              {#if agentQueue.length > QUEUE_CAP}
                <p class="px-4 py-2 text-caption text-[var(--text-faint)] border-t border-[var(--border)]">
                  {t("home.queue.more", { count: agentQueue.length - QUEUE_CAP })}
                </p>
              {/if}
            {/if}
          </section>

          <section
            class="rounded-xl bg-[var(--surface)] border border-[var(--border)] overflow-hidden min-w-0"
            data-testid="needs-you"
          >
            {@render cardHeader(
              User,
              t("home.attention.title"),
              t("home.attention.subtitle"),
              attention ? attentionTotal : undefined,
            )}
            {#if !attention}
              <p class="px-4 py-3 text-body-sm text-[var(--text-muted)]">{t("home.attention.error")}</p>
            {:else if attentionGroups.length === 0}
              <p class="px-4 py-3 text-body-sm text-[var(--text-muted)]">
                {t("home.attention.empty")}
              </p>
            {:else}
              {#each attentionGroups as group (group.key)}
                {@const expanded = attentionExpanded.has(group.key)}
                {@const shown = expanded ? group.items : group.items.slice(0, ATTENTION_CAP)}
                <div data-attention-group={group.key} class="border-b border-[var(--border)] last:border-b-0">
                  <div class="flex items-center gap-2 px-4 pt-2.5 pb-1 text-caption font-medium text-[var(--text-muted)]">
                    <group.icon size={13} class="shrink-0 text-[var(--text-faint)]" />
                    <span>{group.label}</span>
                    <span class="text-micro text-[var(--text-faint)] tabular-nums">{group.items.length}</span>
                  </div>
                  {#each shown as issue (issue.id)}
                    <button
                      class="w-full flex items-center gap-2.5 px-4 py-2 text-left hover:bg-[var(--bg-subtle)] transition-colors"
                      data-identifier={issue.identifier}
                      onclick={() => openIssue(issue)}
                    >
                      <StatusIcon status={issue.status} size={14} />
                      <span class="text-caption font-mono text-[var(--text-faint)] w-[64px] shrink-0 truncate">
                        {issue.identifier}
                      </span>
                      <span class="text-body-sm text-[var(--text)] truncate flex-1">{issue.title}</span>
                      <PriorityIcon priority={issue.priority} size={15} />
                    </button>
                  {/each}
                  {#if group.items.length > ATTENTION_CAP}
                    <button
                      class="w-full flex items-center justify-center gap-1 px-4 py-2
                             text-caption text-[var(--text-muted)] hover:text-[var(--text)]
                             hover:bg-[var(--bg-subtle)] transition-colors"
                      onclick={() => toggleAttentionGroup(group.key)}
                    >
                      {expanded ? t("home.showFewer") : t("home.showAll", { count: group.items.length })}
                    </button>
                  {/if}
                </div>
              {/each}
            {/if}
            {@render sectionLink("/for-me", t("home.attention.link"))}
          </section>
        </div>

        <!-- ── ACTIVITY, HEATMAP, PINS ──────────────────────── -->
        <div class="grid grid-cols-1 lg:grid-cols-[1fr_340px] gap-6 items-start">
          <section class="rounded-xl bg-[var(--surface)] border border-[var(--border)] p-4 min-w-0">
            <div class="flex items-center justify-between mb-2">
              <h2 class="text-body-sm font-semibold text-[var(--text)]">{t("home.activity.title")}</h2>
              {#if realtimeActivityReady}
                <span
                  class="text-micro text-[var(--text-faint)] tabular-nums text-right"
                  title={t("home.rate.title")}
                >
                  {rateLabel(activityRate)}
                </span>
              {/if}
            </div>
            {#if activityItems.length === 0}
              <p class="text-body-sm text-[var(--text-muted)]">{t("home.activity.empty")}</p>
            {:else}
              <div class="grid grid-cols-1 md:grid-cols-2 gap-x-4 gap-y-0.5">
                {#each activityItems as a (a.id)}
                  {@const dest = activityDest(a)}
                  {@const ident = projectIdent(a.project_id)}
                  <button
                    type="button"
                    disabled={!dest}
                    class="flex items-start gap-2 px-2 py-1.5 -mx-2 rounded-md text-body-sm leading-snug text-left
                           {dest ? 'hover:bg-[var(--bg-subtle)]' : ''}
                           transition-colors disabled:cursor-default min-w-0"
                    onclick={() => dest && navigate(dest)}
                  >
                    {#if a.actor_is_bot}
                      <Bot size={13} class="shrink-0 mt-0.5 text-[var(--text-faint)]" />
                    {:else}
                      <User size={13} class="shrink-0 mt-0.5 text-[var(--text-faint)]" />
                    {/if}
                    <span class="flex-1 min-w-0 text-[var(--text-muted)]">
                      <span class="font-medium text-[var(--text)]">{activityActor(a)}</span>
                      {activityVerb(a)}
                      {#if ident}
                        <span class="font-mono text-caption text-[var(--accent)]">
                          {a.entity_label ?? `${ident} #${a.entity_id}`}
                        </span>
                      {/if}
                    </span>
                  </button>
                {/each}
              </div>
            {/if}
            {@render sectionLink("/feed", t("home.activity.link"), true)}
          </section>

          <aside class="flex flex-col gap-6 min-w-0">
            {#if overview}
              <section class="rounded-xl bg-[var(--surface)] border border-[var(--border)] p-4">
                <DoneHeatmap days={overview.my_done} />
              </section>
            {/if}

            {#if pinnedPages.length > 0}
              <section>
                <div class="flex items-center gap-2 mb-2">
                  <Pin size={12} class="text-[var(--text-faint)]" />
                  <h2 class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]">
                    {t("home.pinned")}
                  </h2>
                </div>
                <div class="flex flex-col gap-0.5">
                  {#each pinnedPages as page (page.id)}
                    {@const ident = projectIdent(page.project_id)}
                    {#if ident}
                      <button
                        class="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-left
                               hover:bg-[var(--bg-subtle)] transition-colors"
                        onclick={() => navigate(`/${ident}/pages/${page.id}`)}
                      >
                        <FileText size={13} class="shrink-0 text-[var(--text-faint)]" />
                        <span class="flex-1 min-w-0 text-body-sm text-[var(--text)] truncate">
                          {page.title}
                        </span>
                        <span class="shrink-0 text-micro text-[var(--text-faint)] font-mono">
                          {ident}
                        </span>
                      </button>
                    {/if}
                  {/each}
                </div>
              </section>
            {/if}
          </aside>
        </div>
      </div>
    {/if}
  </div>
</div>
