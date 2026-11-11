<script lang="ts">
  // For me (fork): open issues a person has to act on, across every project
  // the reader can see, as vertical cards with quick actions (comment,
  // status, reassign, cancel). Filters and sorting run on the server
  // (/api/for-me) and persist per browser. Cards handed over since the
  // reader last looked are tinted. Like the Feed, the issue preview takes the
  // filter rail's place on wide screens.

  import { getContext, tick, untrack } from "svelte";
  import { fly, fade } from "svelte/transition";
  import {
    listForMe,
    listProjects,
    listProjectGroups,
    listUsers,
    createComment,
    me,
    type ForMeItem,
    type ForMeParams,
    type ForMeScope,
    type ForMeRequestedBy,
    type Project,
    type ProjectGroup,
    type UserSummary,
  } from "../lib/api";
  import {
    loadFilters,
    saveFilters,
    activeFilterCount,
    loadSeen,
    saveSeen,
    isNew,
    plainText,
    SORT_OPTIONS,
    SCOPE_OPTIONS,
    STATUS_OPTIONS,
    PRIORITY_OPTIONS,
    DEFAULT_FILTERS,
    type ForMeFilters,
  } from "../lib/forMe";
  import { selectedProjectIds, toggle, statusLabel, initials } from "../lib/feed";
  import { startAutoRefresh } from "../lib/autoRefresh.svelte";
  import { loadScroll, clearScroll, scrollSaver } from "../lib/scrollMemory";
  import { openPeek, peekState, dockPeek, registerPeekSync } from "../lib/issues/peek.svelte";
  import { updateIssueWithUndo } from "../lib/issues/state.svelte";
  import PeekPanel from "../lib/issues/PeekPanel.svelte";
  import { toast } from "../lib/toast/toast.svelte";
  import StatusIcon from "../lib/StatusIcon.svelte";
  import PriorityIcon from "../lib/PriorityIcon.svelte";
  import ProjectIcon from "../lib/ProjectIcon.svelte";
  import TimeAgo from "../lib/TimeAgo.svelte";
  import Skeleton from "../lib/Skeleton.svelte";
  import ErrorState from "../lib/ErrorState.svelte";
  import {
    MessageSquare, SlidersHorizontal, X, SendHorizontal, Search, Bot, UserRound, Users,
    ChevronDown, Check, CircleSlash, UserCheck, Hourglass, Inbox, ArrowUpRight,
  } from "lucide-svelte";

  const PAGE_SIZE = 40;
  const LABEL_PREVIEW = 3;

  let { navigate }: { navigate: (path: string) => void } = $props();

  const topbarCtx = getContext<{
    set: (s: import("svelte").Snippet | undefined) => void;
  } | undefined>("lific:topbar");
  $effect(() => {
    topbarCtx?.set(topbarContent);
    return () => topbarCtx?.set(undefined);
  });

  // ── State ──────────────────────────────────────────────

  let filters = $state<ForMeFilters>(loadFilters());
  let projects = $state<Project[]>([]);
  let groups = $state<ProjectGroup[]>([]);
  let people = $state<UserSummary[]>([]);
  let myUsername = $state<string | null>(null);

  let items = $state<ForMeItem[]>([]);
  let total = $state(0);
  let counts = $state<Record<ForMeScope, number>>({ me: 0, anyone: 0, others: 0 });
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state("");
  let request = 0;

  // "New" is relative to the previous visit; this visit becomes the next
  // baseline. Acting on a card acknowledges it.
  const seenAt = loadSeen();
  saveSeen(Date.now());
  let acknowledged = $state<Set<number>>(new Set());
  /** Cards that arrived while the page was open. */
  let arrived = $state<Set<number>>(new Set());

  let filtersOpen = $state(false);
  let projectQuery = $state("");

  /** Open quick-action menu: `${issueId}:${kind}`. */
  let menu = $state<string | null>(null);
  let composerFor = $state<number | null>(null);
  let draft = $state("");
  let sending = $state(false);

  let sentinel = $state<HTMLDivElement | null>(null);
  let scroller = $state<HTMLDivElement | null>(null);

  // ── Data ───────────────────────────────────────────────

  function params(): ForMeParams {
    return {
      projects: selectedProjectIds(filters, groups),
      statuses: filters.statuses.length ? filters.statuses : undefined,
      priorities: filters.priorities.length ? filters.priorities : undefined,
      scopes: filters.scopes.length ? filters.scopes : undefined,
      requested_by: filters.requestedBy,
      sort: filters.sort,
      mine_first: filters.mineFirst,
    };
  }

  async function loadMeta() {
    const [p, g, u, m] = await Promise.all([listProjects(), listProjectGroups(), listUsers(), me()]);
    if (p.ok) projects = p.data;
    if (g.ok) groups = g.data;
    if (u.ok) people = u.data.filter((x) => x.is_active);
    if (m.ok) myUsername = m.data.username;
  }

  // Coming back restores the cards and scroll position (per tab); a filter
  // change starts from the top.
  const SCROLL_KEY = "for-me";
  let firstLoad = true;
  const rememberScroll = scrollSaver(SCROLL_KEY, () =>
    scroller ? { top: scroller.scrollTop, count: items.length } : null,
  );

  async function load() {
    const token = ++request;
    loading = true;
    error = "";
    const restore = firstLoad ? loadScroll(SCROLL_KEY) : null;
    if (!firstLoad) clearScroll(SCROLL_KEY);
    firstLoad = false;
    const want = restore ? Math.min(Math.max(restore.count, PAGE_SIZE), 200) : PAGE_SIZE;
    const res = await listForMe({ ...params(), limit: want });
    if (token !== request) return;
    if (res.ok) apply(res.data, false);
    else error = res.error;
    loading = false;
    await tick();
    // A restored visit goes back to its place; new filters start at the top.
    if (scroller) scroller.scrollTop = restore && res.ok ? restore.top : 0;
  }

  function apply(page: { items: ForMeItem[]; total: number; has_more: boolean; counts: Record<ForMeScope, number> }, live: boolean) {
    if (live) {
      const known = new Set(items.map((i) => i.id));
      const fresh = page.items.filter((i) => !known.has(i.id)).map((i) => i.id);
      if (fresh.length) arrived = new Set([...arrived, ...fresh]);
    }
    items = page.items;
    total = page.total;
    hasMore = page.has_more;
    counts = page.counts;
  }

  async function loadMore() {
    if (loadingMore || !hasMore) return;
    const token = request;
    loadingMore = true;
    const res = await listForMe({ ...params(), offset: items.length, limit: PAGE_SIZE });
    if (token === request && res.ok) {
      const known = new Set(items.map((i) => i.id));
      items = [...items, ...res.data.items.filter((i) => !known.has(i.id))];
      hasMore = res.data.has_more;
      total = res.data.total;
      counts = res.data.counts;
    }
    loadingMore = false;
  }

  /** Re-read everything on screen in place (realtime, after an action). */
  async function refresh() {
    if (loading) return;
    const token = request;
    const res = await listForMe({ ...params(), limit: Math.max(items.length, PAGE_SIZE) });
    if (token === request && res.ok) apply(res.data, true);
  }

  $effect(() => {
    void loadMeta();
  });

  $effect(() => {
    void JSON.stringify(filters);
    if (filters.groups.length) void groups;
    saveFilters($state.snapshot(filters) as ForMeFilters);
    // Only the filters above trigger a reload: `load` itself reads groups
    // and the list, and tracking those would reload (and forget the saved
    // scroll position) whenever the group list arrives.
    untrack(() => void load());
  });

  $effect(() =>
    startAutoRefresh({
      refresh,
      intervalMs: 30_000,
      isBusy: () => loading || loadingMore || menu !== null || composerFor !== null,
      shouldRefresh: (event) =>
        event.type === "resync.required" || typeof event.project_id === "number",
      realtimeDebounceMs: 400,
      realtimeMaxWaitMs: 3000,
    }),
  );

  // Edits made in the preview land back on the cards.
  $effect(() => registerPeekSync(() => void refresh()));

  // Wide screens: the preview replaces the filter rail.
  $effect(() => {
    const wide = window.matchMedia("(min-width: 1024px)");
    let undock: (() => void) | null = null;
    const sync = () => {
      if (wide.matches && !undock) undock = dockPeek();
      else if (!wide.matches && undock) {
        undock();
        undock = null;
      }
    };
    sync();
    wide.addEventListener("change", sync);
    return () => {
      wide.removeEventListener("change", sync);
      undock?.();
    };
  });
  let previewing = $derived(peekState.docked && peekState.open);

  $effect(() => {
    if (!sentinel || !scroller) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) void loadMore();
      },
      { root: scroller, rootMargin: "600px" },
    );
    io.observe(sentinel);
    return () => io.disconnect();
  });

  // ── Derived ────────────────────────────────────────────

  let filterCount = $derived(activeFilterCount(filters));
  let shownProjects = $derived(
    projectQuery.trim()
      ? projects.filter((p) =>
          `${p.identifier} ${p.name}`.toLowerCase().includes(projectQuery.trim().toLowerCase()),
        )
      : projects,
  );
  let projectEmoji = $derived(new Map(projects.map((p) => [p.id, p.emoji])));
  let freshCount = $derived(items.filter((i) => highlighted(i)).length);

  function highlighted(item: ForMeItem): boolean {
    return !acknowledged.has(item.id) && (arrived.has(item.id) || isNew(item, seenAt));
  }

  // ── Filters ────────────────────────────────────────────

  function setFilters(patch: Partial<ForMeFilters>) {
    filters = { ...filters, ...patch };
  }

  function resetFilters() {
    filters = { ...DEFAULT_FILTERS, sort: filters.sort, mineFirst: filters.mineFirst };
  }

  // ── Quick actions ──────────────────────────────────────

  function acknowledge(item: ForMeItem) {
    if (!acknowledged.has(item.id)) acknowledged = new Set([...acknowledged, item.id]);
  }

  function open(item: ForMeItem) {
    acknowledge(item);
    openPeek(item.identifier);
  }

  function toggleMenu(item: ForMeItem, kind: string) {
    const key = `${item.id}:${kind}`;
    menu = menu === key ? null : key;
  }

  function assignmentNames(item: ForMeItem): string[] {
    if (!item.needs_human) return [];
    return item.assignees?.length ? item.assignees.map((a) => a.username) : ["human"];
  }

  async function mutate(item: ForMeItem, patch: Record<string, unknown>, prevPatch: Record<string, unknown>) {
    menu = null;
    acknowledge(item);
    await updateIssueWithUndo({
      id: item.id,
      identifier: item.identifier,
      patch,
      prevPatch,
      modules: [],
      people: people.map((p) => ({ user_id: p.id, username: p.username, display_name: p.display_name })),
      onApplied: () => void refresh(),
    });
  }

  function setStatus(item: ForMeItem, status: string) {
    if (status !== item.status) void mutate(item, { status }, { status: item.status });
  }

  function reassign(item: ForMeItem, names: string[]) {
    void mutate(item, { assignees: names }, { assignees: assignmentNames(item) });
  }

  async function openComposer(item: ForMeItem) {
    menu = null;
    acknowledge(item);
    if (composerFor !== item.id) draft = "";
    composerFor = composerFor === item.id ? null : item.id;
    await tick();
    document.querySelector<HTMLTextAreaElement>(`[data-composer="${item.id}"]`)?.focus();
  }

  async function sendComment(item: ForMeItem) {
    const body = draft.trim();
    if (!body || sending) return;
    sending = true;
    const res = await createComment(item.id, body);
    sending = false;
    if (!res.ok) {
      toast(`Couldn't post comment: ${res.error}`);
      return;
    }
    draft = "";
    composerFor = null;
    toast(`Comment posted on ${item.identifier}`);
    void refresh();
  }

  function composerKey(e: KeyboardEvent, item: ForMeItem) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void sendComment(item);
    } else if (e.key === "Escape") {
      composerFor = null;
    }
  }

  function autoGrow(node: HTMLTextAreaElement) {
    const fit = () => {
      node.style.height = "auto";
      node.style.height = Math.min(node.scrollHeight, 280) + "px";
    };
    node.addEventListener("input", fit);
    fit();
    return { destroy: () => node.removeEventListener("input", fit) };
  }

  function onWindowClick(e: MouseEvent) {
    if (menu && !(e.target as HTMLElement).closest("[data-fm-menu]")) menu = null;
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key === "Escape" && menu) menu = null;
  }

  function assignmentLabel(item: ForMeItem): string {
    if (item.mine && item.assignees?.length === 1) return "You";
    if (item.assignees?.length) {
      return item.assignees.map((a) => (a.username === myUsername ? "you" : a.display_name || a.username)).join(", ");
    }
    if (item.needs_human) return "Any person";
    return item.waiting_on_me ? "Agents" : "Nobody";
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKey} />

{#snippet sectionLabel(text: string, extra?: string)}
  <div class="flex items-center gap-2 mb-2">
    <span class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]">{text}</span>
    {#if extra}<span class="text-micro text-[var(--text-faint)] tabular-nums">{extra}</span>{/if}
  </div>
{/snippet}

{#snippet pill(on: boolean, onclick: () => void, content: import("svelte").Snippet)}
  <button
    class="inline-flex items-center gap-1.5 text-caption px-2 py-1 rounded-full border transition-colors
           {on
      ? 'border-[var(--accent)] bg-[var(--accent-subtle)] text-[var(--accent)] font-medium'
      : 'border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)] hover:border-[var(--text-faint)]'}"
    {onclick}
  >{@render content()}</button>
{/snippet}

{#snippet filterPanel()}
  <div class="flex flex-col gap-6">
    <section>
      {@render sectionLabel("Sort")}
      <div class="flex flex-col gap-0.5">
        {#each SORT_OPTIONS as o (o.value)}
          {@const on = filters.sort === o.value}
          <button
            class="flex items-center gap-2 px-2 py-1.5 rounded-md text-left text-body-sm transition-colors
                   {on ? 'bg-[var(--accent-subtle)] text-[var(--accent)] font-medium' : 'text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
            onclick={() => setFilters({ sort: o.value })}
          >
            <span class="size-3.5 grid place-items-center">{#if on}<Check size={13} />{/if}</span>{o.label}
          </button>
        {/each}
      </div>
      <!-- Mine-first only reorders, so it sits with sorting. -->
      <button
        class="mt-2 w-full flex items-center justify-between gap-3 px-2 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] text-left"
        role="switch"
        aria-checked={filters.mineFirst}
        onclick={() => setFilters({ mineFirst: !filters.mineFirst })}
      >
        <span class="text-body-sm text-[var(--text)]">Assigned to me first</span>
        <span
          class="relative w-8 h-[18px] rounded-full transition-colors shrink-0
                 {filters.mineFirst ? 'bg-[var(--accent)]' : 'bg-[var(--border)]'}"
        >
          <span
            class="absolute top-[2px] size-[14px] rounded-full bg-white shadow transition-[left]
                   {filters.mineFirst ? 'left-[16px]' : 'left-[2px]'}"
          ></span>
        </span>
      </button>
    </section>

    <section>
      {@render sectionLabel("Whose")}
      <div class="flex flex-wrap gap-1.5">
        {#each SCOPE_OPTIONS as o (o.value)}
          {#snippet c()}{o.label}<span class="tabular-nums text-[var(--text-faint)]">{counts[o.value]}</span>{/snippet}
          <span title={o.hint}>{@render pill(filters.scopes.includes(o.value), () => setFilters({ scopes: toggle(filters.scopes, o.value) }), c)}</span>
        {/each}
      </div>
    </section>

    <section>
      {@render sectionLabel("Requested by")}
      <div class="grid grid-cols-3 p-0.5 rounded-lg bg-[var(--bg-subtle)] border border-[var(--border)] text-caption">
        {#each [["all", "Anyone"], ["agents", "Agents"], ["people", "People"]] as [v, label] (v)}
          <button
            class="py-1 rounded-md transition-colors inline-flex items-center justify-center gap-1
                   {filters.requestedBy === v ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm font-medium' : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
            onclick={() => setFilters({ requestedBy: v as ForMeRequestedBy })}
          >{#if v === "agents"}<Bot size={12} />{:else if v === "people"}<Users size={12} />{/if}{label}</button>
        {/each}
      </div>
    </section>

    <section>
      {@render sectionLabel("Status", filters.statuses.length ? undefined : "open")}
      <div class="flex flex-wrap gap-1.5">
        {#each STATUS_OPTIONS as st (st)}
          {#snippet c()}<StatusIcon status={st} size={12} />{statusLabel(st)}{/snippet}
          {@render pill(filters.statuses.includes(st), () => setFilters({ statuses: toggle(filters.statuses, st) }), c)}
        {/each}
      </div>
    </section>

    <section>
      {@render sectionLabel("Priority")}
      <div class="flex flex-wrap gap-1.5">
        {#each PRIORITY_OPTIONS as pr (pr)}
          {#snippet c()}<PriorityIcon priority={pr} size={12} /><span class="capitalize">{pr}</span>{/snippet}
          {@render pill(filters.priorities.includes(pr), () => setFilters({ priorities: toggle(filters.priorities, pr) }), c)}
        {/each}
      </div>
    </section>

    {#if groups.length}
      <section>
        {@render sectionLabel("Groups")}
        <div class="flex flex-wrap gap-1.5">
          {#each groups as g (g.id)}
            {#snippet c()}{g.name}<span class="text-[var(--text-faint)] tabular-nums">{g.project_ids.length}</span>{/snippet}
            {@render pill(filters.groups.includes(g.id), () => setFilters({ groups: toggle(filters.groups, g.id) }), c)}
          {/each}
        </div>
      </section>
    {/if}

    <section>
      {@render sectionLabel("Projects", filters.projects.length ? `${filters.projects.length} selected` : undefined)}
      {#if projects.length > 8}
        <label class="mb-1.5 flex items-center gap-2 px-2 py-1 rounded-md border border-[var(--border)] focus-within:border-[var(--accent)]">
          <Search size={12} class="text-[var(--text-faint)]" />
          <input class="flex-1 min-w-0 bg-transparent outline-none text-body-sm" placeholder="Find project" bind:value={projectQuery} />
        </label>
      {/if}
      <div class="flex flex-col gap-0.5">
        {#each shownProjects as p (p.id)}
          {@const on = filters.projects.includes(p.id)}
          <button
            class="flex items-center gap-2 px-2 py-1.5 rounded-md text-left transition-colors
                   {on ? 'bg-[var(--accent-subtle)]' : 'hover:bg-[var(--bg-subtle)]'}"
            onclick={() => setFilters({ projects: toggle(filters.projects, p.id) })}
          >
            <ProjectIcon value={p.emoji} size={14} />
            <span class="flex-1 min-w-0 truncate text-body-sm {on ? 'text-[var(--accent)] font-medium' : 'text-[var(--text)]'}">{p.name}</span>
            <span class="text-micro font-mono text-[var(--text-faint)]">{p.identifier}</span>
          </button>
        {/each}
      </div>
    </section>

    {#if filterCount}
      <button class="self-start text-caption text-[var(--text-muted)] hover:text-[var(--text)] inline-flex items-center gap-1" onclick={resetFilters}>
        <X size={12} />Reset filters
      </button>
    {/if}
  </div>
{/snippet}

{#snippet menuShell(content: import("svelte").Snippet)}
  <div
    class="absolute z-30 left-0 top-full mt-1 min-w-[190px] max-h-[280px] overflow-y-auto rounded-lg border border-[var(--border)]
           bg-[var(--surface)] shadow-lg py-1"
    transition:fly={{ y: -4, duration: 120 }}
  >{@render content()}</div>
{/snippet}

{#snippet menuItem(label: string, onclick: () => void, current: boolean, icon: import("svelte").Snippet)}
  <button
    class="w-full flex items-center gap-2 px-2.5 py-1.5 text-left text-body-sm hover:bg-[var(--bg-subtle)]
           {current ? 'text-[var(--accent)] font-medium' : 'text-[var(--text)]'}"
    {onclick}
  >
    <span class="size-4 grid place-items-center shrink-0">{@render icon()}</span>
    <span class="flex-1 truncate">{label}</span>
    {#if current}<Check size={12} />{/if}
  </button>
{/snippet}

{#snippet actionButton(label: string, onclick: () => void, active: boolean, icon: import("svelte").Snippet, danger = false)}
  <button
    class="inline-flex items-center gap-1 text-caption px-1.5 py-1 rounded-md transition-colors
           {active
      ? 'text-[var(--text)] bg-[var(--bg-subtle)]'
      : danger
        ? 'text-[var(--text-faint)] hover:text-[var(--error)] hover:bg-[var(--error-bg)]'
        : 'text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
    title={label}
    {onclick}
  >{@render icon()}<span class="fm-action-label">{label}</span></button>
{/snippet}

{#snippet card(item: ForMeItem)}
  {@const fresh = highlighted(item)}
  {@const labels = item.labels ?? []}
  {@const previewed = previewing && peekState.identifier === item.identifier}
  <article
    class="fm-card relative flex flex-col rounded-xl border transition-colors
           {fresh
      ? 'fm-new border-[color-mix(in_oklab,var(--accent)_55%,var(--border))]'
      : previewed ? 'border-[var(--accent)] bg-[var(--surface)]' : 'border-[var(--border)] bg-[var(--surface)] hover:border-[color-mix(in_oklab,var(--text-faint)_60%,var(--border))]'}"
    in:fly={{ y: 6, duration: 200 }}
    out:fade={{ duration: 150 }}
  >
    <div class="flex flex-col gap-2 p-3.5 pb-2.5 flex-1">
      <!-- Where it lives, how urgent, how fresh. -->
      <header class="flex items-center gap-1.5 min-w-0 text-caption">
        <span class="shrink-0"><ProjectIcon value={projectEmoji.get(item.project_id) ?? null} size={13} /></span>
        <button class="font-mono font-medium text-[var(--text-muted)] hover:text-[var(--accent)]" onclick={() => open(item)}>{item.identifier}</button>
        <span class="truncate text-[var(--text-faint)]">{item.project_name}</span>
        <span class="ml-auto flex items-center gap-1.5 shrink-0">
          {#if fresh}
            <span class="text-micro font-semibold uppercase tracking-wider px-1.5 py-px rounded bg-[var(--accent)] text-[var(--accent-text)]">New</span>
          {/if}
          {#if item.priority !== "none"}
            <span title="Priority: {item.priority}"><PriorityIcon priority={item.priority} size={14} /></span>
          {/if}
        </span>
      </header>

      <button class="text-left text-body font-medium leading-snug text-[var(--text)] line-clamp-2 hover:underline decoration-[var(--border)] underline-offset-2" onclick={() => open(item)}>
        {item.title}
      </button>

      {#if item.description}
        <p class="m-0 text-caption leading-relaxed text-[var(--text-muted)] line-clamp-3">{plainText(item.description, 320)}</p>
      {/if}

      {#if item.last_comment}
        <div class="rounded-lg bg-[var(--bg-subtle)] px-2.5 py-1.5">
          <div class="flex items-center gap-1.5 text-micro text-[var(--text-faint)]">
            {#if item.last_comment.is_bot}<Bot size={11} />{:else}<UserRound size={11} />{/if}
            <span class="font-medium text-[var(--text-muted)] truncate">{item.last_comment.author}</span>
            <TimeAgo class="ml-auto shrink-0" date={item.last_comment.created_at} />
          </div>
          <p class="m-0 mt-0.5 text-caption leading-snug text-[var(--text-muted)] line-clamp-2">{plainText(item.last_comment.content, 240)}</p>
        </div>
      {/if}

      <!-- Status (a quick action too), labels, waits. -->
      <div class="flex flex-wrap items-center gap-1.5 mt-auto pt-0.5">
        <span class="inline-flex items-center gap-1 text-micro font-medium px-1.5 py-0.5 rounded-full border border-[var(--border)] text-[var(--text)]">
          <StatusIcon status={item.status} size={11} />{statusLabel(item.status)}
        </span>
        {#if item.waiting_on_me}
          <span class="inline-flex items-center gap-1 text-micro font-medium px-1.5 py-0.5 rounded-full bg-[color-mix(in_oklab,var(--warn)_15%,transparent)] text-[var(--warn-text)]">
            <Hourglass size={10} />Waiting on you
          </span>
        {/if}
        {#each labels.slice(0, LABEL_PREVIEW) as l (l)}
          <span class="text-micro px-1.5 py-0.5 rounded-full border border-[var(--border)] text-[var(--text-muted)]">{l}</span>
        {/each}
        {#if labels.length > LABEL_PREVIEW}
          <span class="text-micro text-[var(--text-faint)]">+{labels.length - LABEL_PREVIEW}</span>
        {/if}
      </div>

      <!-- Who it is for, who asked, when. -->
      <div class="flex items-center gap-2 text-micro text-[var(--text-faint)] min-w-0">
        <span class="inline-flex items-center gap-1 shrink-0 {item.mine ? 'text-[var(--accent)] font-medium' : ''}">
          <UserCheck size={11} />{assignmentLabel(item)}
        </span>
        {#if item.assigned_by}
          <span class="truncate">· from {item.assigned_by.name}{item.assigned_by.is_bot ? " (agent)" : ""}</span>
        {/if}
        <span class="ml-auto shrink-0 flex items-center gap-2">
          {#if item.comment_count}
            <span class="inline-flex items-center gap-0.5"><MessageSquare size={11} />{item.comment_count}</span>
          {/if}
          <TimeAgo date={item.assigned_at} />
        </span>
      </div>
    </div>

    <!-- Quick actions. -->
    <footer class="flex items-center gap-0.5 px-2 py-1.5 border-t border-[var(--border)]" data-fm-menu>
      {#snippet ic1()}<MessageSquare size={13} />{/snippet}
      {@render actionButton("Comment", () => openComposer(item), composerFor === item.id, ic1)}

      <div class="relative">
        {#snippet ic2()}<StatusIcon status={item.status} size={13} />{/snippet}
        {@render actionButton("Status", () => toggleMenu(item, "status"), menu === `${item.id}:status`, ic2)}
        {#if menu === `${item.id}:status`}
          {#snippet statusMenu()}
            {#each STATUS_OPTIONS.filter((s) => s !== "cancelled") as st (st)}
              {#snippet i()}<StatusIcon status={st} size={13} />{/snippet}
              {@render menuItem(statusLabel(st), () => setStatus(item, st), item.status === st, i)}
            {/each}
          {/snippet}
          {@render menuShell(statusMenu)}
        {/if}
      </div>

      <div class="relative">
        {#snippet ic3()}<UserRound size={13} />{/snippet}
        {@render actionButton("Reassign", () => toggleMenu(item, "assign"), menu === `${item.id}:assign`, ic3)}
        {#if menu === `${item.id}:assign`}
          {@const names = assignmentNames(item)}
          {#snippet assignMenu()}
            {#if myUsername}
              {#snippet im()}<UserCheck size={13} />{/snippet}
              {@render menuItem("Me", () => reassign(item, [myUsername!]), names.length === 1 && names[0] === myUsername, im)}
            {/if}
            {#snippet ia()}<Users size={13} />{/snippet}
            {@render menuItem("Any person", () => reassign(item, ["human"]), names.length === 1 && names[0] === "human", ia)}
            {#each people.filter((p) => p.username !== myUsername) as p (p.id)}
              {#snippet ip()}<span class="size-4 rounded-full grid place-items-center text-[8px] font-bold bg-[var(--accent)] text-[var(--accent-text)]">{initials(p.display_name || p.username)}</span>{/snippet}
              {@render menuItem(p.display_name || p.username, () => reassign(item, [p.username]), names.length === 1 && names[0] === p.username, ip)}
            {/each}
            <div class="my-1 h-px bg-[var(--border)]"></div>
            {#snippet ib()}<Bot size={13} />{/snippet}
            {@render menuItem("Hand back to agents", () => reassign(item, []), names.length === 0, ib)}
          {/snippet}
          {@render menuShell(assignMenu)}
        {/if}
      </div>

      <span class="ml-auto flex items-center gap-0.5">
        {#snippet ic4()}<CircleSlash size={13} />{/snippet}
        {@render actionButton("Cancel", () => setStatus(item, "cancelled"), false, ic4, true)}
        <button
          class="hidden sm:grid size-6 place-items-center rounded-md text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]"
          title="Open" aria-label="Open {item.identifier}"
          onclick={() => navigate(`/${item.project_identifier}/issues/${item.identifier}`)}
        ><ArrowUpRight size={13} /></button>
      </span>
    </footer>

    {#if composerFor === item.id}
      <div class="mx-2 mb-2 rounded-lg border border-[var(--border)] bg-[var(--bg)] focus-within:border-[var(--accent)] transition-colors" transition:fly={{ y: -4, duration: 150 }}>
        <textarea
          data-composer={item.id}
          class="block w-full resize-none bg-transparent px-2.5 py-1.5 text-body-sm text-[var(--text)] placeholder:text-[var(--text-faint)] outline-none min-h-[56px]"
          placeholder="Comment on {item.identifier}… Markdown works"
          bind:value={draft}
          use:autoGrow
          onkeydown={(e) => composerKey(e, item)}
          disabled={sending}
        ></textarea>
        <div class="flex items-center justify-end gap-1.5 px-1.5 pb-1.5">
          <span class="hidden sm:inline mr-auto pl-1 text-micro text-[var(--text-faint)]">⌘/Ctrl + Enter · Esc</span>
          <button class="text-caption px-2 py-0.5 rounded-md text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]" onclick={() => (composerFor = null)}>Cancel</button>
          <button
            class="inline-flex items-center gap-1 text-caption font-medium px-2 py-0.5 rounded-md text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)] disabled:opacity-50"
            disabled={!draft.trim() || sending}
            onclick={() => sendComment(item)}
          ><SendHorizontal size={11} />{sending ? "Sending…" : "Send"}</button>
        </div>
      </div>
    {/if}
  </article>
{/snippet}

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-4 sm:px-6 py-2 w-full">
    <span class="hidden sm:inline text-body-sm font-medium text-[var(--text)]">For me</span>
    {#if !loading}
      <span class="text-micro text-[var(--text-faint)] tabular-nums">{total}</span>
    {/if}
    {#if freshCount}
      <span class="text-micro font-medium px-1.5 py-px rounded-full bg-[var(--accent-subtle)] text-[var(--accent)]">{freshCount} new</span>
    {/if}
    <div class="ml-auto flex items-center gap-2">
      {#if filterCount}
        <button class="hidden lg:inline-flex items-center gap-1 text-caption text-[var(--text-muted)] hover:text-[var(--text)]" onclick={resetFilters}>
          <X size={12} />Reset
        </button>
      {/if}
      <button
        class="lg:hidden inline-flex items-center gap-1.5 text-caption px-2 py-1 rounded-md border transition-colors
               {filtersOpen || filterCount ? 'border-[var(--accent)] text-[var(--accent)]' : 'border-[var(--border)] text-[var(--text-muted)]'}"
        onclick={() => (filtersOpen = !filtersOpen)}
      ><SlidersHorizontal size={12} />Filters{filterCount ? ` · ${filterCount}` : ""}</button>
    </div>
  </div>
{/snippet}

<div class="h-full flex flex-col">
  <div class="flex-1 overflow-y-auto" bind:this={scroller} onscroll={rememberScroll}>
    <div class="flex flex-col lg:flex-row gap-8 px-3 sm:px-6 lg:px-8 py-5 max-w-[1320px] mx-auto items-start">
      {#if filtersOpen}
        <div class="lg:hidden w-full rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4" transition:fly={{ y: -6, duration: 150 }}>
          {@render filterPanel()}
        </div>
      {/if}

      <div class="flex-1 min-w-0 w-full">
        {#if loading}
          <div class="grid grid-cols-1 sm:grid-cols-2 2xl:grid-cols-3 gap-3">
            {#each Array(6) as _, i (i)}
              <div class="rounded-xl border border-[var(--border)] p-3.5 flex flex-col gap-2.5">
                <Skeleton variant="bar" class="h-3 w-24" />
                <Skeleton variant="bar" class="h-4 w-4/5" />
                <Skeleton variant="bar" class="h-3 w-full" />
                <Skeleton variant="bar" class="h-3 w-2/3" />
                <Skeleton variant="bar" class="h-8 w-full mt-2" />
              </div>
            {/each}
          </div>
        {:else if error}
          <ErrorState title="Couldn't load your work" message={error}>
            <button
              class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
              onclick={load}
            >Try again</button>
          </ErrorState>
        {:else if items.length === 0}
          <div class="flex flex-col items-center py-20 gap-3 px-6 max-w-[440px] mx-auto text-center">
            <Inbox size={32} class="text-[var(--text-faint)]" />
            {#if filterCount}
              <p class="text-body-lg text-[var(--text-muted)] m-0">Nothing matches these filters</p>
              <button class="text-caption text-[var(--accent)] hover:underline" onclick={resetFilters}>Reset filters</button>
            {:else}
              <p class="text-body-lg text-[var(--text-muted)] m-0">Nothing needs a person right now</p>
              <p class="text-body-sm text-[var(--text-faint)] m-0">Issues an agent hands to a person, or that wait on you, show up here.</p>
            {/if}
          </div>
        {:else}
          <div class="grid grid-cols-1 sm:grid-cols-2 {previewing ? '' : '2xl:grid-cols-3'} gap-3 items-stretch">
            {#each items as item (item.id)}
              {@render card(item)}
            {/each}
          </div>
          <div bind:this={sentinel} class="h-px"></div>
          {#if loadingMore}
            <p class="text-center text-caption text-[var(--text-faint)] py-4 m-0">Loading…</p>
          {/if}
        {/if}
      </div>

      {#if previewing}
        <aside class="hidden lg:block w-[400px] xl:w-[440px] shrink-0 sticky top-5 h-[calc(100vh-7rem)]">
          <PeekPanel {navigate} embedded />
        </aside>
      {:else}
        <aside class="fm-rail hidden lg:block w-[272px] shrink-0 sticky top-5 max-h-[calc(100vh-7rem)] overflow-y-auto pb-6">
          {@render filterPanel()}
        </aside>
      {/if}
    </div>
  </div>
</div>

<style>
  /* Handed over since the last visit: tinted until acted on. */
  .fm-new {
    background: color-mix(in oklab, var(--accent) 7%, var(--surface));
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--accent) 25%, transparent);
  }
  .fm-rail { scrollbar-width: none; }
  .fm-rail::-webkit-scrollbar { display: none; }
  /* Narrow cards keep the action icons and drop their words. */
  .fm-card { container-type: inline-size; }
  @container (max-width: 340px) {
    .fm-action-label { display: none; }
  }
</style>
