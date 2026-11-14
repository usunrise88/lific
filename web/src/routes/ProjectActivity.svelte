<script lang="ts">
  // LIF-158 — project activity feed, full surface.
  //
  // Two-pane layout: the feed (day-grouped, expandable rows) plus an
  // actor rail showing who's been doing what — humans and agents ranked
  // by action count, each a one-click filter. Expanding a row reveals
  // the full record: exact timestamps (local + UTC), transport, the
  // actor's standing in this project, and complete old → new values.
  // Background-polls every 15s so an agent working over MCP is visible
  // in near-real-time.

  import {
    listProjects,
    listProjectActivity,
    listProjectActivityActors,
    type Project,
    type Activity,
    type ActorStat,
  } from "../lib/api";
  import StatusIcon from "../lib/StatusIcon.svelte";
  import PriorityIcon from "../lib/PriorityIcon.svelte";
  import { startAutoRefresh } from "../lib/autoRefresh.svelte";
  import { formatDate } from "../lib/format";
  import TimeAgo from "../lib/TimeAgo.svelte";
  import {
    ChevronRight, ChevronDown, History, ArrowUpRight,
    CircleDot, FileText, MessageSquare, Layers, Tag, FolderClosed, Box,
  } from "lucide-svelte";
  import ErrorState from "../lib/ErrorState.svelte";
  import Skeleton from "../lib/Skeleton.svelte";
  import {
    diffLines,
    foldContext,
    isUnchanged,
    type DiffRow,
  } from "../lib/linediff";
  import { getContext } from "svelte";
  import { t, locale, formatNumber } from "../lib/i18n";
  import { relationLabel } from "../lib/feed";

  const PAGE_SIZE = 50;

  const topbarCtx = getContext<{
    set: (s: import("svelte").Snippet | undefined) => void;
  } | undefined>("lific:topbar");

  $effect(() => {
    topbarCtx?.set(topbarContent);
    return () => topbarCtx?.set(undefined);
  });

  let {
    navigate,
    projectIdentifier,
  }: {
    navigate: (path: string) => void;
    projectIdentifier: string;
  } = $props();

  let project = $state<Project | null>(null);
  let items = $state<Activity[]>([]);
  let actors = $state<ActorStat[]>([]);
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state("");

  // Row expansion (one at a time — the expanded card is the focus).
  let expandedId = $state<number | null>(null);

  // Actor filter: undefined = everyone, null = system bucket, number = user.
  let filterActor = $state<number | null | undefined>(undefined);

  $effect(() => {
    const id = projectIdentifier;
    filterActor = undefined;
    expandedId = null;
    loadProject(id);
  });

  async function loadProject(ident: string) {
    loading = true;
    error = "";
    items = [];
    actors = [];
    const projRes = await listProjects();
    if (!projRes.ok) { error = projRes.error; loading = false; return; }
    const found = projRes.data.find((p) => p.identifier === ident);
    if (!found) { error = t("projectActivity.notFound", { identifier: ident }); loading = false; return; }
    project = found;

    const [feedRes, actorRes] = await Promise.all([
      listProjectActivity(found.id, PAGE_SIZE, 0),
      listProjectActivityActors(found.id),
    ]);
    if (feedRes.ok) {
      items = feedRes.data.items;
      hasMore = feedRes.data.has_more;
    }
    if (actorRes.ok) actors = actorRes.data;
    loading = false;
  }

  async function loadMore() {
    if (!project || loadingMore) return;
    loadingMore = true;
    const res = await listProjectActivity(project.id, PAGE_SIZE, items.length);
    if (res.ok) {
      const known = new Set(items.map((a) => a.id));
      items = [...items, ...res.data.items.filter((a) => !known.has(a.id))];
      hasMore = res.data.has_more;
    }
    loadingMore = false;
  }

  // Background poll: prepend fresh entries + refresh actor counts.
  async function refreshFeed() {
    if (!project) return;
    const [feedRes, actorRes] = await Promise.all([
      listProjectActivity(project.id, PAGE_SIZE, 0),
      listProjectActivityActors(project.id),
    ]);
    if (feedRes.ok) {
      const known = new Set(items.map((a) => a.id));
      const fresh = feedRes.data.items.filter((a) => !known.has(a.id));
      if (fresh.length > 0) items = [...fresh, ...items];
      if (items.length <= PAGE_SIZE) hasMore = feedRes.data.has_more;
    }
    if (actorRes.ok) actors = actorRes.data;
  }

  $effect(() =>
    startAutoRefresh({
      refresh: refreshFeed,
      isBusy: () => loading || loadingMore,
      shouldRefresh: (event) =>
        event.type === "resync.required" ||
        (typeof event.project_id === "number" && event.project_id === project?.id),
    }),
  );

  // ── Filtering + day grouping ─────────────────────────

  let filtered = $derived(
    filterActor === undefined
      ? items
      : items.filter((a) => a.actor_user_id === filterActor),
  );

  let dayGroups = $derived.by(() => {
    const groups: { label: string; entries: Activity[] }[] = [];
    let currentKey = "";
    for (const a of filtered) {
      const d = new Date(a.ts + "Z");
      const key = d.toDateString();
      if (key !== currentKey) {
        currentKey = key;
        groups.push({ label: dayLabel(d), entries: [] });
      }
      groups[groups.length - 1].entries.push(a);
    }
    return groups;
  });

  function dayLabel(d: Date): string {
    const now = new Date();
    if (d.toDateString() === now.toDateString()) return t("common.today");
    const yesterday = new Date(now.getTime() - 86_400_000);
    if (d.toDateString() === yesterday.toDateString()) return t("common.yesterday");
    return d.toLocaleDateString(locale(), {
      weekday: "short",
      month: "short",
      day: "numeric",
      ...(d.getFullYear() !== now.getFullYear() ? { year: "numeric" } : {}),
    });
  }

  // ── Presentation helpers ─────────────────────────────

  function actorName(a: Activity): string {
    return a.actor_display_name || a.actor_username || t("projectActivity.system");
  }

  function shortValue(v: string | null, max = 60): string {
    if (!v) return t("projectActivity.noneValue");
    const flat = v.replace(/\n+/g, " ").trim();
    if (!flat) return t("projectActivity.noneValue");
    return flat.length > max ? flat.slice(0, max) + "…" : flat;
  }

  const ENTITY_TYPES = ["issue", "page", "comment", "module", "label", "folder", "plan", "project"];

  /** Display name of an entity type (lowercase, as it reads mid-sentence). */
  function entityName(type: string): string {
    return ENTITY_TYPES.includes(type) ? t(`projectActivity.entity.${type}`) : type;
  }

  // The phrase between the actor and the entity ("{actor} created issue LIF-1").
  function verb(a: Activity): string {
    switch (a.action) {
      case "create":
        return a.entity_type === "comment" ? t("projectActivity.verb.commented") : t("projectActivity.verb.created", { entity: entityName(a.entity_type) });
      case "delete":
        return a.entity_type === "comment" ? t("projectActivity.verb.deletedComment") : t("projectActivity.verb.deleted", { entity: entityName(a.entity_type) });
      case "update":
        return a.entity_type === "comment" ? t("projectActivity.verb.editedComment") : t("projectActivity.verb.changed", { field: String(a.field) });
      case "attach":
        return t("projectActivity.verb.labeled");
      case "detach":
        return t("projectActivity.verb.unlabeled");
      case "link":
        return t("projectActivity.verb.linked", { relation: relationLabel(a.field) });
      case "unlink":
        return t("projectActivity.verb.unlinked", { relation: relationLabel(a.field) });
      default:
        return a.action;
    }
  }

  const ISSUE_STATUSES = ["backlog", "todo", "active", "done", "cancelled"];
  const PRIORITIES = ["urgent", "high", "medium", "low", "none"];
  function statusName(v: string | null): string {
    return v && ISSUE_STATUSES.includes(v) ? t(`common.status.${v}`) : (v ?? "");
  }
  function priorityName(v: string | null): string {
    return v && PRIORITIES.includes(v) ? t(`common.priority.${v}`) : (v ?? "");
  }

  function entityDestination(a: Activity): string | null {
    switch (a.entity_type) {
      case "issue":
        return a.entity_label ? `/${projectIdentifier}/issues/${a.entity_label}` : null;
      case "page":
        return `/${projectIdentifier}/pages/${a.entity_id}`;
      case "module":
        return `/${projectIdentifier}/modules/${a.entity_id}`;
      case "comment":
        if (a.issue_id !== null && a.entity_label) {
          return `/${projectIdentifier}/issues/${a.entity_label}`;
        }
        if (a.page_id !== null) return `/${projectIdentifier}/pages/${a.page_id}`;
        return null;
      default:
        return null;
    }
  }

  /** Stats for the expanded entry's actor, with rank among project actors. */
  function actorStanding(a: Activity): { stat: ActorStat; rank: number } | null {
    const idx = actors.findIndex((s) => s.actor_user_id === a.actor_user_id);
    if (idx < 0) return null;
    return { stat: actors[idx], rank: idx + 1 };
  }

  function ordinal(n: number): string {
    const cat = new Intl.PluralRules(locale(), { type: "ordinal" }).select(n);
    const form = cat === "one" || cat === "two" || cat === "few" ? cat : "other";
    return t(`projectActivity.ordinal.${form}`, { n });
  }

  function actorStatName(s: ActorStat): string {
    return s.display_name || s.username || t("projectActivity.system");
  }

  function initials(name: string): string {
    return name
      .split(/[\s_-]+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? "")
      .join("");
  }

  let maxActions = $derived(actors.reduce((m, s) => Math.max(m, s.actions), 1));

  // ── Value diffing ────────────────────────────────────
  //
  // Description and content edits are multi-line, and showing the whole old
  // value in red above the whole new value in green buried a one-word change
  // in a wall of text. Those get a line diff instead. Single-line fields
  // (status, priority, title) keep the two-block rendering, and so does any
  // document too large to diff cheaply. Only the expanded row is diffed.

  type ValueDiff = { rows: DiffRow[]; unchanged: boolean };

  let expandedDiff = $derived.by<ValueDiff | null>(() => {
    if (expandedId === null) return null;
    const a = items.find((entry) => entry.id === expandedId);
    if (!a) return null;
    const oldText = a.old_value ?? "";
    const newText = a.new_value ?? "";
    if (!oldText.includes("\n") && !newText.includes("\n")) return null;
    const lines = diffLines(oldText, newText);
    if (lines === null) return null; // too large: fall back to whole values
    return { rows: foldContext(lines), unchanged: isUnchanged(lines) };
  });

  function toggleExpand(id: number) {
    expandedId = expandedId === id ? null : id;
  }

  function toggleActorFilter(s: ActorStat) {
    const key = s.actor_user_id;
    filterActor = filterActor === key ? undefined : key;
    expandedId = null;
  }
</script>

{#snippet entityIcon(type: string, size: number)}
  {#if type === "issue"}
    <CircleDot {size} class="shrink-0 text-[var(--text-faint)]" />
  {:else if type === "page"}
    <FileText {size} class="shrink-0 text-[var(--text-faint)]" />
  {:else if type === "comment"}
    <MessageSquare {size} class="shrink-0 text-[var(--text-faint)]" />
  {:else if type === "module"}
    <Layers {size} class="shrink-0 text-[var(--text-faint)]" />
  {:else if type === "label"}
    <Tag {size} class="shrink-0 text-[var(--text-faint)]" />
  {:else if type === "folder"}
    <FolderClosed {size} class="shrink-0 text-[var(--text-faint)]" />
  {:else}
    <Box {size} class="shrink-0 text-[var(--text-faint)]" />
  {/if}
{/snippet}

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-6 py-2 w-full">
    <div class="flex items-center gap-1.5 shrink-0">
      <button
        class="text-body-sm font-mono font-medium text-[var(--text-muted)]
               hover:text-[var(--text)] transition-colors"
        onclick={() => navigate(`/${projectIdentifier}/overview`)}
      >
        {projectIdentifier}
      </button>
      <ChevronRight size={12} class="text-[var(--text-faint)]" />
      <span class="text-body-sm font-medium text-[var(--text)]">
        {t("projectActivity.title")}
      </span>
      {#if !loading}
        <span
          class="ml-1 text-micro text-[var(--text-faint)] font-medium
                 tabular-nums"
        >
          {filtered.length}{hasMore && filterActor === undefined ? "+" : ""}
        </span>
      {/if}
    </div>

    {#if filterActor !== undefined}
      {@const active = actors.find((s) => s.actor_user_id === filterActor)}
      <div class="flex items-center gap-1.5">
        <span
          class="flex items-center gap-1.5 text-caption font-medium
                 text-[var(--accent)] bg-[var(--accent-subtle)]
                 pl-2.5 pr-1 py-0.5 rounded-full"
        >
          {t("projectActivity.filter.only", { name: active ? actorStatName(active) : "?" })}
          <button
            class="size-4 flex items-center justify-center rounded-full
                   hover:bg-[var(--accent)] hover:text-[var(--accent-text)]
                   transition-colors"
            title={t("projectActivity.filter.clear")}
            onclick={() => { filterActor = undefined; }}
          >
            ×
          </button>
        </span>
      </div>
    {/if}
  </div>
{/snippet}

<div class="h-full flex flex-col">
  <div class="flex-1 overflow-y-auto">
    {#if loading}
      <!-- LIF-281: feed-shaped skeleton. Mirrors the loaded two-pane frame
           (px-8 py-6 max-w-[1280px] wrapper, lg:w-[260px] actor rail), the
           day header (label + count + divider rule), the gap-2.5 px-2.5 py-1.5
           entity rows with a 14px leading icon, and the actor-rail rows
           (px-2.5 py-2, size-6 avatar + two text lines + trailing count).
           Corrects drift from the earlier LIF-246 skeleton (rail was 280px,
           header lacked the divider, rows lacked padding + trailing count). -->
      <div class="flex flex-col lg:flex-row gap-8 px-8 py-6 max-w-[1280px] mx-auto items-start">
        <div class="flex-1 min-w-0 w-full">
          <!-- Day header: label + count + divider, matching the sticky row. -->
          <div class="-mx-2 px-2 py-1.5 mb-1 flex items-center gap-2">
            <Skeleton variant="bar" class="h-2.5 w-20" />
            <Skeleton variant="bar" class="h-2.5 w-4" />
            <div class="flex-1 h-px bg-[var(--border)]"></div>
          </div>
          <div class="flex flex-col">
            {#each Array(6) as _, i (i)}
              <div class="flex items-center gap-2.5 px-2.5 py-1.5">
                <Skeleton variant="circle" class="size-3.5" />
                <Skeleton variant="bar" class="h-3 flex-1 max-w-[420px]" />
                <Skeleton variant="bar" class="h-2.5 w-8 shrink-0" />
              </div>
            {/each}
          </div>
        </div>
        <aside class="w-full lg:w-[260px] shrink-0">
          <div class="flex items-center gap-2 mb-3">
            <Skeleton variant="bar" class="h-2.5 w-16" />
            <Skeleton variant="bar" class="h-2.5 w-4" />
          </div>
          <div class="flex flex-col gap-0.5">
            {#each Array(4) as _, i (i)}
              <div class="flex items-center gap-2.5 px-2.5 py-2">
                <Skeleton variant="circle" class="size-6 shrink-0" />
                <div class="flex-1 min-w-0 flex flex-col gap-1.5">
                  <Skeleton variant="bar" class="h-3 w-24" />
                  <Skeleton variant="bar" class="h-2 w-32" />
                </div>
                <Skeleton variant="bar" class="h-2.5 w-6 shrink-0" />
              </div>
            {/each}
          </div>
        </aside>
      </div>
    {:else if error}
      <ErrorState title={t("projectActivity.loadError")} message={error}>
        <button
          class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
          onclick={() => loadProject(projectIdentifier)}
        >
          {t("common.retry")}
        </button>
        <button
          class="text-body-sm text-[var(--text-muted)] border border-[var(--border)] px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] transition-colors"
          onclick={() => navigate(`/${projectIdentifier}/overview`)}
        >
          {t("projectActivity.projectOverview")}
        </button>
      </ErrorState>
    {:else if items.length === 0}
      <div class="flex flex-col items-center py-20 gap-3 px-6 max-w-[480px] mx-auto text-center">
        <History size={32} class="text-[var(--text-faint)]" />
        <p class="text-body-lg text-[var(--text-muted)]">{t("projectActivity.empty.title")}</p>
        <p class="text-body-sm text-[var(--text-faint)] leading-relaxed">
          {t("projectActivity.empty.body")}
        </p>
      </div>
    {:else}
      <div class="flex flex-col lg:flex-row gap-8 px-8 py-6 max-w-[1280px] mx-auto items-start">
        <!-- ── FEED ──────────────────────────────────────── -->
        <div class="flex-1 min-w-0 w-full">
          {#each dayGroups as group (group.label)}
            <div class="mb-6 last:mb-0">
              <!-- Day header: sticky so long days keep their context. -->
              <div
                class="sticky top-0 z-10 -mx-2 px-2 py-1.5 mb-1
                       bg-[var(--bg)] flex items-center gap-2"
              >
                <span
                  class="text-micro font-semibold uppercase tracking-widest
                         text-[var(--text-muted)]"
                >
                  {group.label}
                </span>
                <span class="text-micro text-[var(--text-faint)] tabular-nums">
                  {group.entries.length}
                </span>
                <div class="flex-1 h-px bg-[var(--border)]"></div>
              </div>

              {#each group.entries as a (a.id)}
                {@const isOpen = expandedId === a.id}
                {@const dest = entityDestination(a)}
                <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                <div
                  class="rounded-md transition-colors
                         {isOpen
                    ? 'bg-[var(--surface)] border border-[var(--border)] shadow-[0_1px_3px_rgba(0,0,0,0.05)] my-1.5'
                    : 'hover:bg-[var(--bg-subtle)] border border-transparent'}"
                >
                  <div
                    class="flex items-center gap-2.5 px-2.5 py-1.5 cursor-pointer"
                    role="button"
                    tabindex="0"
                    onclick={() => toggleExpand(a.id)}
                  >
                    {@render entityIcon(a.entity_type, 14)}

                    <div class="flex-1 min-w-0 text-body-sm leading-relaxed text-[var(--text-muted)] truncate">
                      <span class="font-medium text-[var(--text)]">{actorName(a)}</span>
                      {#if a.actor_is_bot}
                        <span
                          class="inline-block align-middle text-micro font-semibold
                                 uppercase tracking-wider px-1 py-px rounded
                                 bg-[var(--accent-subtle)] text-[var(--accent)] mx-0.5"
                        >
                          {t("projectActivity.agent")}
                        </span>
                      {/if}
                      {verb(a)}
                      {#if dest}
                        <button
                          class="font-mono text-caption text-[var(--accent)]
                                 hover:underline"
                          onclick={(e) => { e.stopPropagation(); navigate(dest); }}
                        >
                          {a.entity_label ?? `#${a.entity_id}`}
                        </button>
                      {:else}
                        <span class="font-mono text-caption">
                          {a.entity_label ?? `#${a.entity_id}`}
                        </span>
                      {/if}

                      <!-- Inline value summary -->
                      {#if a.action === "update" && a.field === "status"}
                        <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                          <StatusIcon status={a.old_value ?? ""} size={12} />
                        </span>
                        <span class="text-[var(--text-faint)]">→</span>
                        <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                          <StatusIcon status={a.new_value ?? ""} size={12} />
                          <span class="text-[var(--text)]">{statusName(a.new_value)}</span>
                        </span>
                      {:else if a.action === "update" && a.field === "priority"}
                        <span class="text-[var(--text-faint)]">→</span>
                        <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                          <PriorityIcon priority={a.new_value ?? "none"} size={12} />
                          <span class="text-[var(--text)]">{priorityName(a.new_value)}</span>
                        </span>
                      {:else if a.action === "attach" || a.action === "detach"}
                        <span
                          class="text-micro font-medium px-1.5 py-0.5 rounded-full
                                 border border-[var(--border)] align-middle"
                        >
                          {a.action === "attach" ? a.new_value : a.old_value}
                        </span>
                      {:else if a.action === "link" || a.action === "unlink"}
                        <span class="font-mono text-caption text-[var(--accent)]">
                          {a.action === "link" ? a.new_value : a.old_value}
                        </span>
                      {:else if a.action === "create" && a.entity_type === "comment"}
                        <span class="text-[var(--text-faint)] italic">
                          {t("projectActivity.quoted", { text: shortValue(a.new_value, 48) })}
                        </span>
                      {:else if a.action === "update" && a.field !== "description" && a.field !== "content"}
                        <span class="text-[var(--text-faint)]">{shortValue(a.old_value, 24)}</span>
                        <span class="text-[var(--text-faint)]">→</span>
                        <span class="text-[var(--text)]">{shortValue(a.new_value, 24)}</span>
                      {/if}
                    </div>

                    <TimeAgo
                      class="shrink-0 text-micro text-[var(--text-faint)] tabular-nums"
                      date={a.ts}
                    />
                    <ChevronDown
                      size={12}
                      class="shrink-0 text-[var(--text-faint)] transition-transform
                             {isOpen ? 'rotate-180' : ''}"
                    />
                  </div>

                  <!-- ── Expanded record ──────────────────── -->
                  {#if isOpen}
                    {@const standing = actorStanding(a)}
                    <div class="px-4 pb-3.5 pt-1 border-t border-[var(--border)] mx-2.5 mb-1">
                      <div class="grid grid-cols-1 sm:grid-cols-2 gap-x-8 gap-y-3 pt-2.5">
                        <!-- When -->
                        <div>
                          <p class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1">
                            {t("projectActivity.record.when")}
                          </p>
                          <p class="text-body-sm text-[var(--text)] m-0">
                            {formatDate(a.ts)}
                          </p>
                          <p class="text-micro font-mono text-[var(--text-faint)] m-0 mt-0.5">
                            {t("projectActivity.record.utc", { ts: a.ts })}
                          </p>
                        </div>

                        <!-- Who -->
                        <div>
                          <p class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1">
                            {t("projectActivity.record.who")}
                          </p>
                          <p class="text-body-sm text-[var(--text)] m-0">
                            {actorName(a)}
                            {#if a.actor_username && a.actor_display_name && a.actor_username !== a.actor_display_name}
                              <span class="text-[var(--text-faint)]">({a.actor_username})</span>
                            {/if}
                            {#if a.actor_is_bot}
                              <span
                                class="inline-block align-middle text-micro font-semibold
                                       uppercase tracking-wider px-1 py-px rounded
                                       bg-[var(--accent-subtle)] text-[var(--accent)] ml-1"
                              >
                                {t("projectActivity.agent")}
                              </span>
                            {/if}
                            <span class="text-[var(--text-muted)]">{t("projectActivity.via", { transport: a.transport })}</span>
                          </p>
                          {#if standing}
                            <p class="text-micro text-[var(--text-muted)] m-0 mt-0.5">
                              {t("projectActivity.record.standing.before", { actions: t("projectActivity.record.actions", { count: standing.stat.actions }), rank: ordinal(standing.rank) })}
                              <TimeAgo date={standing.stat.last_ts} />
                            </p>
                          {/if}
                        </div>

                        <!-- What -->
                        <div class="sm:col-span-2">
                          <p class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1">
                            {t("projectActivity.record.what")}
                          </p>
                          <p class="text-body-sm text-[var(--text)] m-0 flex items-center gap-1.5 flex-wrap">
                            {@render entityIcon(a.entity_type, 13)}
                            <span class="capitalize">{entityName(a.entity_type)}</span>
                            <span class="font-mono text-caption text-[var(--text-muted)]">
                              {a.entity_label ?? `#${a.entity_id}`}
                            </span>
                            <span class="text-[var(--text-muted)]">— {a.action}{a.field ? ` · ${a.field}` : ""}</span>
                            {#if dest}
                              <button
                                class="inline-flex items-center gap-0.5 text-caption
                                       text-[var(--accent)] hover:underline"
                                onclick={(e) => { e.stopPropagation(); navigate(dest); }}
                              >
                                {t("common.open")} <ArrowUpRight size={11} />
                              </button>
                            {/if}
                          </p>
                        </div>

                        <!-- Values: a line diff for multi-line text, full
                             old → new blocks for everything else -->
                        {#if a.old_value !== null || a.new_value !== null}
                          <div class="sm:col-span-2 flex flex-col gap-1.5">
                            {#if expandedDiff}
                              <!-- Line diff: red for removed, green for added,
                                   plain for context, with long unchanged runs
                                   folded behind a divider. -->
                              <div
                                class="text-caption leading-relaxed rounded-md
                                       border border-[var(--border)] overflow-hidden
                                       max-h-[320px] overflow-y-auto"
                              >
                                {#if expandedDiff.unchanged}
                                  <p class="px-3 py-2 m-0 text-[var(--text-faint)] italic">
                                    {t("projectActivity.diff.noChanges")}
                                  </p>
                                {/if}
                                {#each expandedDiff.rows as row, i (i)}
                                  {#if row.kind === "fold"}
                                    <div
                                      class="flex items-center gap-2 px-3 py-1
                                             bg-[var(--bg-subtle)] select-none"
                                    >
                                      <span class="h-px flex-1 bg-[var(--border)]"></span>
                                      <span class="text-micro text-[var(--text-faint)] tabular-nums">
                                        {t("projectActivity.diff.unchanged", { count: row.count })}
                                      </span>
                                      <span class="h-px flex-1 bg-[var(--border)]"></span>
                                    </div>
                                  {:else}
                                    <div
                                      class="flex gap-2 px-3 min-h-[1.5em]
                                             {row.kind === 'removed'
                                        ? 'bg-[var(--error-bg)] text-[var(--text-muted)]'
                                        : row.kind === 'added'
                                          ? 'bg-[var(--success-bg)] text-[var(--text)]'
                                          : 'text-[var(--text-muted)]'}"
                                    >
                                      <span
                                        class="shrink-0 w-3 font-mono select-none
                                               text-[var(--text-faint)]"
                                        aria-hidden="true"
                                      >{row.kind === "removed" ? "-" : row.kind === "added" ? "+" : ""}</span>
                                      <span
                                        class="flex-1 min-w-0 whitespace-pre-wrap break-words"
                                      >{row.text}</span>
                                    </div>
                                  {/if}
                                {/each}
                              </div>
                            {:else if a.old_value !== null}
                              <div
                                class="text-caption leading-relaxed px-3 py-2 rounded-md
                                       border border-[var(--border)] bg-[var(--error-bg)]
                                       text-[var(--text-muted)] whitespace-pre-wrap break-words
                                       max-h-[240px] overflow-y-auto"
                              >{a.old_value}</div>
                            {/if}
                            {#if !expandedDiff && a.new_value !== null}
                              <div
                                class="text-caption leading-relaxed px-3 py-2 rounded-md
                                       border border-[var(--border)] bg-[var(--success-bg)]
                                       text-[var(--text)] whitespace-pre-wrap break-words
                                       max-h-[240px] overflow-y-auto"
                              >{a.new_value}</div>
                            {/if}
                          </div>
                        {/if}
                      </div>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/each}

          {#if hasMore && filterActor === undefined}
            <button
              class="mt-2 text-caption text-[var(--text-muted)]
                     hover:text-[var(--text)] inline-flex items-center gap-1
                     transition-colors px-2.5 py-1 rounded-md
                     hover:bg-[var(--bg-subtle)]"
              disabled={loadingMore}
              onclick={loadMore}
            >
              <ChevronDown size={12} />
              {loadingMore ? t("projectActivity.loadingMore") : t("projectActivity.loadMore")}
            </button>
          {/if}
        </div>

        <!-- ── ACTOR RAIL ────────────────────────────────── -->
        <aside class="w-full lg:w-[260px] shrink-0 lg:sticky lg:top-6">
          <div class="flex items-center gap-2 mb-3">
            <span
              class="text-micro font-semibold uppercase tracking-widest
                     text-[var(--text-muted)]"
            >
              {t("projectActivity.actors")}
            </span>
            <span class="text-micro text-[var(--text-faint)] tabular-nums">
              {actors.length}
            </span>
          </div>

          <div class="flex flex-col gap-0.5">
            {#each actors as s (s.actor_user_id)}
              {@const name = actorStatName(s)}
              {@const active = filterActor === s.actor_user_id}
              <button
                class="relative text-left px-2.5 py-2 rounded-md transition-colors
                       overflow-hidden
                       {active
                  ? 'bg-[var(--accent-subtle)]'
                  : 'hover:bg-[var(--bg-subtle)]'}"
                title={active ? t("projectActivity.rail.titleClear", { transport: s.top_transport }) : t("projectActivity.rail.titleShow", { name, transport: s.top_transport })}
                onclick={() => toggleActorFilter(s)}
              >
                <div class="flex items-center gap-2.5">
                  <span
                    class="size-6 rounded-full flex items-center justify-center
                           text-micro font-bold shrink-0 select-none
                           {s.is_bot
                      ? 'bg-[var(--accent-subtle)] text-[var(--accent)] border border-[var(--accent)]'
                      : 'bg-[var(--accent)] text-[var(--accent-text)]'}"
                  >
                    {initials(name)}
                  </span>
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-1.5">
                      <span class="text-body-sm text-[var(--text)] truncate font-medium">
                        {name}
                      </span>
                      {#if s.is_bot}
                        <span
                          class="text-micro font-semibold uppercase tracking-wider
                                 px-1 py-px rounded bg-[var(--accent-subtle)]
                                 text-[var(--accent)] shrink-0"
                        >
                          {t("projectActivity.agent")}
                        </span>
                      {/if}
                    </div>
                    <div class="text-micro text-[var(--text-faint)]">
                      {t("projectActivity.rail.via.before", { transport: s.top_transport })} <TimeAgo date={s.last_ts} />
                    </div>
                  </div>
                  <span class="text-caption text-[var(--text-muted)] tabular-nums shrink-0">
                    {formatNumber(s.actions)}
                  </span>
                </div>
                <!-- Relative-volume bar along the bottom edge. -->
                <span
                  class="absolute bottom-0 left-2.5 h-[2px] rounded-full
                         bg-[var(--accent)] opacity-30"
                  style="width: calc({Math.max(4, (s.actions / maxActions) * 100)}% - 1.25rem)"
                  aria-hidden="true"
                ></span>
              </button>
            {/each}
          </div>
        </aside>
      </div>
    {/if}
  </div>
</div>
